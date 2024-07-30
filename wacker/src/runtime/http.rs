use crate::runtime::{
    host::Host,
    logs::LogStream,
    read, {Engine, ProgramMeta},
};
use anyhow::{anyhow, bail, Result};
use async_trait::async_trait;
use hyper::Request;
use std::fs::File;
use std::io::Write;
use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc,
};
use wasmtime::{
    component::{Component, Linker, ResourceTable},
    Config, InstanceAllocationStrategy, Memory, MemoryType, PoolingAllocationConfig, Store,
};
use wasmtime_wasi::WasiCtxBuilder;
use wasmtime_wasi_http::{
    bindings::{http::types::Scheme, ProxyPre},
    body::HyperOutgoingBody,
    io::TokioIo,
    WasiHttpCtx, WasiHttpView,
};

#[derive(Clone)]
pub struct HttpEngine {
    engine: wasmtime::Engine,
}

impl HttpEngine {
    pub fn new(config: &Config) -> Result<Self> {
        let mut config = config.clone();
        if use_pooling_allocator_by_default().unwrap_or(false) {
            let pooling_config = PoolingAllocationConfig::default();
            config.allocation_strategy(InstanceAllocationStrategy::Pooling(pooling_config));
        }

        Ok(Self {
            engine: wasmtime::Engine::new(&config)?,
        })
    }

    fn new_store(&self, req_id: u64, stdout: File) -> Result<Store<Host>> {
        let mut builder = WasiCtxBuilder::new();

        let stderr = stdout.try_clone()?;
        builder.stdout(LogStream { output: stdout });
        builder.stderr(LogStream { output: stderr });

        builder.env("REQUEST_ID", req_id.to_string());

        let host = Host {
            table: ResourceTable::new(),
            ctx: builder.build(),
            http: WasiHttpCtx::new(),
        };

        let mut store = Store::new(&self.engine, host);
        store.set_fuel(u64::MAX)?;

        Ok(store)
    }
}

#[async_trait]
impl Engine for HttpEngine {
    async fn run(&self, meta: ProgramMeta, stdout: File) -> Result<()> {
        use hyper::server::conn::http1;

        let mut linker = Linker::new(&self.engine);
        wasmtime_wasi::add_to_linker_async(&mut linker)?;
        wasmtime_wasi_http::add_only_http_to_linker_async(&mut linker)?;

        let bytes = read(&meta.path).await?;
        let component = Component::from_binary(&self.engine, &bytes)?;
        let instance = linker.instantiate_pre(&component)?;
        let instance = ProxyPre::new(instance)?;

        let listener = tokio::net::TcpListener::bind(meta.addr.unwrap()).await?;

        let mut stdout = stdout.try_clone()?;
        stdout.write_fmt(format_args!("Serving HTTP on http://{}/\n", listener.local_addr()?))?;

        let handler = ProxyHandler::new(self.clone(), instance, stdout.try_clone()?);

        loop {
            let (stream, _) = listener.accept().await?;
            let stream = TokioIo::new(stream);
            let h = handler.clone();
            let mut stdout = stdout.try_clone()?;
            tokio::task::spawn(async move {
                if let Err(e) = http1::Builder::new()
                    .keep_alive(true)
                    .serve_connection(
                        stream,
                        hyper::service::service_fn(move |req| handle_request(h.clone(), req)),
                    )
                    .await
                {
                    let _ = stdout.write_fmt(format_args!("serve error: {e:?}\n"));
                }
            });
        }
    }
}

struct ProxyHandlerInner {
    http_engine: HttpEngine,
    instance_pre: ProxyPre<Host>,
    next_id: AtomicU64,
    stdout: File,
}

impl ProxyHandlerInner {
    fn next_req_id(&self) -> u64 {
        self.next_id.fetch_add(1, Ordering::Relaxed)
    }
}

#[derive(Clone)]
struct ProxyHandler(Arc<ProxyHandlerInner>);

impl ProxyHandler {
    fn new(http_engine: HttpEngine, instance_pre: ProxyPre<Host>, stdout: File) -> Self {
        Self(Arc::new(ProxyHandlerInner {
            http_engine,
            instance_pre,
            next_id: AtomicU64::from(0),
            stdout,
        }))
    }
}

async fn handle_request(
    ProxyHandler(inner): ProxyHandler,
    req: Request<hyper::body::Incoming>,
) -> Result<hyper::Response<HyperOutgoingBody>> {
    let (sender, receiver) = tokio::sync::oneshot::channel();

    let req_id = inner.next_req_id();

    let mut stdout = inner.stdout.try_clone()?;
    stdout.write_fmt(format_args!(
        "Request {req_id} handling {} to {}\n",
        req.method(),
        req.uri()
    ))?;

    let mut store = inner.http_engine.new_store(req_id, stdout)?;

    let req = store.data_mut().new_incoming_request(Scheme::Http, req)?;
    let out = store.data_mut().new_response_outparam(sender)?;
    let proxy = inner.instance_pre.instantiate_async(&mut store).await?;

    let task = tokio::task::spawn(async move {
        if let Err(e) = proxy.wasi_http_incoming_handler().call_handle(store, req, out).await {
            log::error!("[{req_id}] :: {:#?}", e);
            return Err(e);
        }

        Ok(())
    });

    match receiver.await {
        Ok(Ok(resp)) => Ok(resp),
        Ok(Err(e)) => Err(e.into()),
        Err(_) => {
            // An error in the receiver (`RecvError`) only indicates that the
            // task exited before a response was sent (i.e., the sender was
            // dropped); it does not describe the underlying cause of failure.
            // Instead we retrieve and propagate the error from inside the task
            // which should more clearly tell the user what went wrong. Note
            // that we assume the task has already exited at this point so the
            // `await` should resolve immediately.
            let e = match task.await {
                Ok(r) => match r {
                    Ok(_) => anyhow!("if the receiver has an error, the task must have failed"),
                    Err(e) => e,
                },
                Err(e) => e.into(),
            };
            bail!("guest never invoked `response-outparam::set` method: {e:?}")
        }
    }
}

// ref: https://github.com/bytecodealliance/wasmtime/blob/ee9e1ca54586516c14d0c4a8dae63691a1d4b50c/src/commands/serve.rs#L561-L597

/// The pooling allocator is tailor made for the `wasmtime serve` use case, so
/// try to use it when we can. The main cost of the pooling allocator, however,
/// is the virtual memory required to run it. Not all systems support the same
/// amount of virtual memory, for example some aarch64 and riscv64 configuration
/// only support 39 bits of virtual address space.
///
/// The pooling allocator, by default, will request 1000 linear memories each
/// sized at 6G per linear memory. This is 6T of virtual memory which ends up
/// being about 42 bits of the address space. This exceeds the 39 bit limit of
/// some systems, so there the pooling allocator will fail by default.
///
/// This function attempts to dynamically determine the hint for the pooling
/// allocator. This returns `Some(true)` if the pooling allocator should be used
/// by default, or `None` or an error otherwise.
///
/// The method for testing this is to allocate a 0-sized 64-bit linear memory
/// with a maximum size that's N bits large where we force all memories to be
/// static. This should attempt to acquire N bits of the virtual address space.
/// If successful that should mean that the pooling allocator is OK to use, but
/// if it fails then the pooling allocator is not used and the normal mmap-based
/// implementation is used instead.
fn use_pooling_allocator_by_default() -> Result<bool> {
    const BITS_TO_TEST: u32 = 42;
    let mut config = Config::new();
    config.wasm_memory64(true);
    config.static_memory_maximum_size(1 << BITS_TO_TEST);
    let engine = wasmtime::Engine::new(&config)?;
    let mut store = Store::new(&engine, ());
    // NB: the maximum size is in wasm pages to take out the 16-bits of wasm
    // page size here from the maximum size.
    let ty = MemoryType::new64(0, Some(1 << (BITS_TO_TEST - 16)));
    Ok(Memory::new(&mut store, ty).is_ok())
}
