use crate::runtime::{
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
use wasmtime::component::{Component, InstancePre, Linker, ResourceTable};
use wasmtime::Store;
use wasmtime_wasi::{bindings, WasiCtx, WasiCtxBuilder, WasiView};
use wasmtime_wasi_http::{
    bindings::http::types as http_types, body::HyperOutgoingBody, hyper_response_error, io::TokioIo, WasiHttpCtx,
    WasiHttpView,
};

struct Host {
    table: ResourceTable,
    ctx: WasiCtx,
    http: WasiHttpCtx,
}

impl WasiView for Host {
    fn table(&mut self) -> &mut ResourceTable {
        &mut self.table
    }

    fn ctx(&mut self) -> &mut WasiCtx {
        &mut self.ctx
    }
}

impl WasiHttpView for Host {
    fn ctx(&mut self) -> &mut WasiHttpCtx {
        &mut self.http
    }

    fn table(&mut self) -> &mut ResourceTable {
        &mut self.table
    }
}

#[derive(Clone)]
pub struct HttpEngine {
    engine: wasmtime::Engine,
}

impl HttpEngine {
    pub fn new(engine: wasmtime::Engine) -> Self {
        Self { engine }
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
            http: WasiHttpCtx,
        };

        let mut store = Store::new(&self.engine, host);
        store.set_fuel(u64::MAX)?;

        Ok(store)
    }

    fn add_to_linker(&self, linker: &mut Linker<Host>) -> Result<()> {
        // ref: https://github.com/bytecodealliance/wasmtime/pull/7728
        bindings::filesystem::preopens::add_to_linker(linker, |t| t)?;
        bindings::filesystem::types::add_to_linker(linker, |t| t)?;
        bindings::cli::environment::add_to_linker(linker, |t| t)?;
        bindings::cli::exit::add_to_linker(linker, |t| t)?;

        wasmtime_wasi_http::proxy::add_to_linker(linker)?;
        Ok(())
    }
}

#[async_trait]
impl Engine for HttpEngine {
    async fn run(&self, meta: ProgramMeta, stdout: File) -> Result<()> {
        use hyper::server::conn::http1;

        let mut linker = Linker::new(&self.engine);
        self.add_to_linker(&mut linker)?;

        let bytes = read(&meta.path).await?;
        let component = Component::from_binary(&self.engine, &bytes)?;
        let instance = linker.instantiate_pre(&component)?;

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
    instance_pre: InstancePre<Host>,
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
    fn new(http_engine: HttpEngine, instance_pre: InstancePre<Host>, stdout: File) -> Self {
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
    use http_body_util::BodyExt;

    let (sender, receiver) = tokio::sync::oneshot::channel();

    let task = tokio::task::spawn(async move {
        let req_id = inner.next_req_id();
        let (mut parts, body) = req.into_parts();

        parts.uri = {
            let uri_parts = parts.uri.into_parts();

            let scheme = uri_parts.scheme.unwrap_or(http::uri::Scheme::HTTP);

            let host = if let Some(val) = parts.headers.get(hyper::header::HOST) {
                std::str::from_utf8(val.as_bytes()).map_err(|_| http_types::ErrorCode::HttpRequestUriInvalid)?
            } else {
                uri_parts
                    .authority
                    .as_ref()
                    .ok_or(http_types::ErrorCode::HttpRequestUriInvalid)?
                    .host()
            };

            let path_with_query = uri_parts
                .path_and_query
                .ok_or(http_types::ErrorCode::HttpRequestUriInvalid)?;

            hyper::Uri::builder()
                .scheme(scheme)
                .authority(host)
                .path_and_query(path_with_query)
                .build()
                .map_err(|_| http_types::ErrorCode::HttpRequestUriInvalid)?
        };

        let req = Request::from_parts(parts, body.map_err(hyper_response_error).boxed());

        let mut stdout = inner.stdout.try_clone()?;
        stdout.write_fmt(format_args!(
            "Request {req_id} handling {} to {}\n",
            req.method(),
            req.uri()
        ))?;

        let mut store = inner.http_engine.new_store(req_id, stdout)?;

        let req = store.data_mut().new_incoming_request(req)?;
        let out = store.data_mut().new_response_outparam(sender)?;

        let (proxy, _) = wasmtime_wasi_http::proxy::Proxy::instantiate_pre(&mut store, &inner.instance_pre).await?;
        proxy.wasi_http_incoming_handler().call_handle(store, req, out).await
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
