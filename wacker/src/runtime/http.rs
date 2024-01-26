use anyhow::{bail, Error, Result};
use std::fs::File;
use std::io::Write;
use std::net::SocketAddr;
use std::pin::Pin;
use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc,
};
use wasmtime::component::{Component, InstancePre, Linker, ResourceTable};
use wasmtime::{Engine, Store};
use wasmtime_wasi::preview2::{self, StreamError, StreamResult, WasiCtx, WasiCtxBuilder, WasiView};
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
    fn table(&self) -> &ResourceTable {
        &self.table
    }

    fn table_mut(&mut self) -> &mut ResourceTable {
        &mut self.table
    }

    fn ctx(&self) -> &WasiCtx {
        &self.ctx
    }

    fn ctx_mut(&mut self) -> &mut WasiCtx {
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
    engine: Engine,
}

impl HttpEngine {
    pub fn new(engine: Engine) -> Self {
        Self { engine }
    }

    fn new_store(&self, req_id: u64, stdout: File) -> Result<Store<Host>> {
        let mut builder = WasiCtxBuilder::new();

        let stderr = stdout.try_clone()?;
        builder.stdout(LogStream { output: stdout });
        builder.stderr(LogStream { output: stderr });

        builder.envs(&[("REQUEST_ID", req_id.to_string())]);

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
        preview2::bindings::filesystem::preopens::add_to_linker(linker, |t| t)?;
        preview2::bindings::filesystem::types::add_to_linker(linker, |t| t)?;
        preview2::bindings::cli::environment::add_to_linker(linker, |t| t)?;
        preview2::bindings::cli::exit::add_to_linker(linker, |t| t)?;

        wasmtime_wasi_http::proxy::add_to_linker(linker)?;
        Ok(())
    }

    pub async fn serve(self, path: &str, addr: SocketAddr, mut stdout: File) -> Result<()> {
        use hyper::server::conn::http1;

        let mut linker = Linker::new(&self.engine);
        self.add_to_linker(&mut linker)?;

        let component = Component::from_file(&self.engine, path)?;
        let instance = linker.instantiate_pre(&component)?;

        let listener = tokio::net::TcpListener::bind(addr).await?;

        stdout.write_all(format!("Serving HTTP on http://{}/\n", listener.local_addr()?).as_bytes())?;

        let handler = ProxyHandler::new(self, instance, stdout);

        loop {
            let (stream, _) = listener.accept().await?;
            let stream = TokioIo::new(stream);
            let h = handler.clone();
            tokio::task::spawn(async move {
                if let Err(e) = http1::Builder::new().keep_alive(true).serve_connection(stream, h).await {
                    eprintln!("error: {e:?}");
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

type Request = hyper::Request<hyper::body::Incoming>;

impl hyper::service::Service<Request> for ProxyHandler {
    type Response = hyper::Response<HyperOutgoingBody>;
    type Error = Error;
    type Future = Pin<Box<dyn std::future::Future<Output = Result<Self::Response>> + Send>>;

    fn call(&self, req: Request) -> Self::Future {
        use http_body_util::BodyExt;

        let ProxyHandler(inner) = self.clone();

        let (sender, receiver) = tokio::sync::oneshot::channel();

        tokio::task::spawn(async move {
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

            let req = hyper::Request::from_parts(parts, body.map_err(hyper_response_error).boxed());

            let mut stdout = inner.stdout.try_clone()?;
            stdout.write_all(format!("Request {req_id} handling {} to {}\n", req.method(), req.uri()).as_bytes())?;

            let mut store = inner.http_engine.new_store(req_id, stdout)?;

            let req = store.data_mut().new_incoming_request(req)?;
            let out = store.data_mut().new_response_outparam(sender)?;

            let (proxy, _inst) =
                wasmtime_wasi_http::proxy::Proxy::instantiate_pre(&mut store, &inner.instance_pre).await?;

            if let Err(e) = proxy.wasi_http_incoming_handler().call_handle(store, req, out).await {
                log::error!("[{req_id}] :: {:#?}", e);
                return Err(e);
            }

            Ok(())
        });

        Box::pin(async move {
            match receiver.await {
                Ok(Ok(resp)) => Ok(resp),
                Ok(Err(e)) => Err(e.into()),
                Err(_) => bail!("guest never invoked `response-outparam::set` method"),
            }
        })
    }
}

struct LogStream {
    output: File,
}

impl preview2::StdoutStream for LogStream {
    fn stream(&self) -> Box<dyn preview2::HostOutputStream> {
        Box::new(LogStream {
            output: self.output.try_clone().expect(""),
        })
    }

    fn isatty(&self) -> bool {
        false
    }
}

impl preview2::HostOutputStream for LogStream {
    fn write(&mut self, bytes: bytes::Bytes) -> StreamResult<()> {
        self.output
            .write_all(bytes.as_ref())
            .map_err(|e| StreamError::LastOperationFailed(Error::from(e)))
    }

    fn flush(&mut self) -> StreamResult<()> {
        Ok(())
    }

    fn check_write(&mut self) -> StreamResult<usize> {
        Ok(1024 * 1024)
    }
}

#[async_trait::async_trait]
impl preview2::Subscribe for LogStream {
    async fn ready(&mut self) {}
}
