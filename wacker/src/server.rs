use crate::config::*;
use crate::module::*;
use crate::runtime::{new_engine, HttpEngine, WasiEngine};
use crate::utils::generate_random_string;
use anyhow::{Error, Result};
use async_stream::try_stream;
use log::{error, info, warn};
use serde::{Deserialize, Serialize};
use sled::Db;
use std::collections::HashMap;
use std::fmt::Display;
use std::fs::{remove_file, OpenOptions};
use std::io::{ErrorKind, SeekFrom, Write};
use std::net::SocketAddr;
use std::path::Path;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::{
    fs::File,
    io::{AsyncReadExt, AsyncSeekExt},
    sync::{mpsc, oneshot, oneshot::error::TryRecvError},
    task, time,
};
use tokio_stream::{wrappers::ReceiverStream, Stream, StreamExt};
use tonic::{Request, Response, Status};

#[derive(Clone)]
pub struct Server {
    db: Db,
    http_engine: HttpEngine,
    wasi_engine: WasiEngine,
    modules: Arc<Mutex<HashMap<String, InnerModule>>>,
    config: Config,
}

struct InnerModule {
    path: String,
    module_type: u32,
    addr: Option<String>,
    receiver: oneshot::Receiver<Error>,
    handler: task::JoinHandle<()>,
    status: u32,
    error: Option<Error>,
}

#[derive(Default, Serialize, Deserialize)]
struct ModuleInDB {
    path: String,
    module_type: u32,
    addr: Option<String>,
}

impl Server {
    pub async fn new(config: Config) -> Result<Self, Error> {
        let db = sled::open(config.db_path.clone())?;
        let engine = new_engine()?;
        let modules = HashMap::new();

        let service = Self {
            db,
            http_engine: HttpEngine::new(engine.clone()),
            wasi_engine: WasiEngine::new(engine),
            modules: Arc::new(Mutex::new(modules)),
            config,
        };
        service.load_from_db().await?;

        Ok(service)
    }

    pub async fn flush_db(&self) -> sled::Result<usize> {
        self.db.flush_async().await
    }

    async fn load_from_db(&self) -> Result<()> {
        for data in self.db.iter() {
            let (id, bytes) = data?;
            let id = String::from_utf8(id.to_vec())?;
            let module: ModuleInDB = bincode::deserialize(&bytes)?;
            match module.module_type {
                MODULE_TYPE_WASI => self.run_inner_wasi(id, module.path).await?,
                MODULE_TYPE_HTTP => self.run_inner_http(id, module.path, module.addr.unwrap()).await?,
                _ => {}
            }
        }
        Ok(())
    }

    async fn run_inner_wasi(&self, id: String, path: String) -> Result<()> {
        let mut modules = self.modules.lock().unwrap();
        let (sender, receiver) = oneshot::channel();
        let engine = self.wasi_engine.clone();

        let mut stdout = OpenOptions::new()
            .create(true)
            .append(true)
            .open(self.config.logs_dir.join(id.clone()))?;
        let stdout_clone = stdout.try_clone()?;

        modules.insert(
            id.clone(),
            InnerModule {
                path: path.clone(),
                module_type: MODULE_TYPE_WASI,
                addr: None,
                receiver,
                handler: task::spawn(async move {
                    match engine.run_wasi(path.clone().as_str(), stdout_clone).await {
                        Ok(_) => {}
                        Err(e) => {
                            error!("running module {} error: {}", id, e);
                            if let Err(file_err) = stdout.write_all(e.to_string().as_bytes()) {
                                warn!("write error log failed: {}", file_err);
                            }
                            if sender.send(e).is_err() {
                                warn!("the receiver dropped");
                            }
                        }
                    }
                }),
                status: MODULE_STATUS_RUNNING,
                error: None,
            },
        );

        Ok(())
    }

    async fn run_inner_http(&self, id: String, path: String, addr: String) -> Result<()> {
        let mut modules = self.modules.lock().unwrap();
        let (sender, receiver) = oneshot::channel();
        let engine = self.http_engine.clone();

        let mut stdout = OpenOptions::new()
            .create(true)
            .append(true)
            .open(self.config.logs_dir.join(id.clone()))?;
        let stdout_clone = stdout.try_clone()?;

        modules.insert(
            id.clone(),
            InnerModule {
                path: path.clone(),
                module_type: MODULE_TYPE_HTTP,
                addr: Option::from(addr.clone()),
                receiver,
                handler: task::spawn(async move {
                    match engine
                        .serve(path.clone().as_str(), addr.parse::<SocketAddr>().unwrap(), stdout_clone)
                        .await
                    {
                        Ok(_) => {}
                        Err(e) => {
                            error!("running module {} error: {}", id, e);
                            if let Err(file_err) = stdout.write_all(e.to_string().as_bytes()) {
                                warn!("write error log failed: {}", file_err);
                            }
                            if sender.send(e).is_err() {
                                warn!("the receiver dropped");
                            }
                        }
                    }
                }),
                status: MODULE_STATUS_RUNNING,
                error: None,
            },
        );

        Ok(())
    }
}

fn to_status<E: Display>(err: E) -> Status {
    Status::internal(err.to_string())
}

#[tonic::async_trait]
impl modules_server::Modules for Server {
    async fn run(&self, request: Request<RunRequest>) -> Result<Response<()>, Status> {
        let req = request.into_inner();

        let file_path = Path::new(&req.path);
        let name = file_path.file_stem();
        if name.is_none() {
            return Err(Status::internal(format!(
                "failed to get file name in path {}",
                req.path
            )));
        }
        let id = format!("{}-{}", name.unwrap().to_str().unwrap(), generate_random_string(7));

        info!("Execute newly added module: {} ({})", id, req.path);

        let module = ModuleInDB {
            path: req.path.clone(),
            module_type: MODULE_TYPE_WASI,
            addr: None,
        };
        match bincode::serialize(&module) {
            Ok(bytes) => {
                self.db.insert(id.as_str(), bytes).map_err(to_status)?;
                self.run_inner_wasi(id, req.path).await.map_err(to_status)?;
                Ok(Response::new(()))
            }
            Err(err) => Err(Status::internal(err.to_string())),
        }
    }

    async fn serve(&self, request: Request<ServeRequest>) -> std::result::Result<Response<()>, Status> {
        let req = request.into_inner();

        let file_path = Path::new(&req.path);
        let name = file_path.file_stem();
        if name.is_none() {
            return Err(Status::internal(format!(
                "failed to get file name in path {}",
                req.path
            )));
        }
        let id = format!("{}-{}", name.unwrap().to_str().unwrap(), generate_random_string(7));

        info!("Serve newly added module: {} ({})", id, req.path);

        let module = ModuleInDB {
            path: req.path.clone(),
            module_type: MODULE_TYPE_HTTP,
            addr: Option::from(req.addr.clone()),
        };
        match bincode::serialize(&module) {
            Ok(bytes) => {
                self.db.insert(id.as_str(), bytes).map_err(to_status)?;
                self.run_inner_http(id, req.path, req.addr).await.map_err(to_status)?;
                Ok(Response::new(()))
            }
            Err(err) => Err(Status::internal(err.to_string())),
        }
    }

    async fn list(&self, _: Request<()>) -> Result<Response<ListResponse>, Status> {
        let mut reply = ListResponse { modules: vec![] };
        let mut modules = self.modules.lock().unwrap();

        for (id, inner) in modules.iter_mut() {
            match inner.status {
                MODULE_STATUS_RUNNING if inner.handler.is_finished() => {
                    inner.status = match inner.receiver.try_recv() {
                        Ok(err) => {
                            inner.error = Option::from(err);
                            MODULE_STATUS_ERROR
                        }
                        Err(TryRecvError::Empty) | Err(TryRecvError::Closed) => MODULE_STATUS_FINISHED,
                    };
                }
                _ => {}
            };

            reply.modules.push(Module {
                id: id.clone(),
                path: inner.path.clone(),
                module_type: inner.module_type,
                status: inner.status,
                addr: inner.addr.clone().unwrap_or_default(),
            });
        }

        Ok(Response::new(reply))
    }

    async fn stop(&self, request: Request<StopRequest>) -> Result<Response<()>, Status> {
        let req = request.into_inner();

        let mut modules = self.modules.lock().unwrap();
        match modules.get_mut(req.id.as_str()) {
            Some(module) => {
                info!("Stop the module: {}", req.id);
                if !module.handler.is_finished() {
                    module.handler.abort();
                    module.status = MODULE_STATUS_STOPPED;
                }
                Ok(Response::new(()))
            }
            None => Err(Status::not_found(format!("module {} not exists", req.id))),
        }
    }

    async fn restart(&self, request: Request<RestartRequest>) -> Result<Response<()>, Status> {
        let req = request.into_inner();
        info!("Restart the module: {}", req.id);

        let (path, module_type, addr) = {
            let modules = self.modules.lock().unwrap();
            let module = modules.get(req.id.as_str());
            if module.is_none() {
                return Err(Status::not_found(format!("module {} not exists", req.id)));
            }

            let module = module.unwrap();
            if !module.handler.is_finished() {
                module.handler.abort();
            }
            (module.path.clone(), module.module_type, module.addr.clone())
        };

        match module_type {
            MODULE_TYPE_WASI => self.run_inner_wasi(req.id, path).await.map_err(to_status)?,
            MODULE_TYPE_HTTP => self
                .run_inner_http(req.id, path, addr.unwrap())
                .await
                .map_err(to_status)?,
            _ => {}
        }
        Ok(Response::new(()))
    }

    async fn delete(&self, request: Request<DeleteRequest>) -> Result<Response<()>, Status> {
        let req = request.into_inner();
        info!("Delete the module: {}", req.id);

        let mut modules = self.modules.lock().unwrap();
        if let Some(module) = modules.get(req.id.as_str()) {
            if !module.handler.is_finished() {
                module.handler.abort();
            }

            if let Err(err) = remove_file(self.config.logs_dir.join(req.id.clone())) {
                if err.kind() != ErrorKind::NotFound {
                    return Err(Status::internal(format!(
                        "failed to remove the log file for {}: {}",
                        req.id.clone(),
                        err
                    )));
                }
            }

            self.db.remove(req.id.clone()).map_err(to_status)?;
            modules.remove(req.id.clone().as_str());
        }

        Ok(Response::new(()))
    }

    type LogsStream = Pin<Box<dyn Stream<Item = Result<LogResponse, Status>> + Send>>;

    async fn logs(&self, request: Request<LogRequest>) -> Result<Response<Self::LogsStream>, Status> {
        let req = request.into_inner();

        let mut file = File::open(self.config.logs_dir.join(req.id)).await?;
        let mut contents = String::new();
        let last_position = file.read_to_string(&mut contents).await?;
        let lines: Vec<&str> = contents.split_inclusive('\n').collect();

        let len = lines.len();
        let mut tail = req.tail as usize;
        if tail == 0 || tail > len {
            tail = len;
        }
        let content = &lines[len - tail..];

        let (tx, rx) = mpsc::channel(128);
        tx.send(Result::<_, Status>::Ok(LogResponse {
            content: content.concat(),
        }))
        .await
        .map_err(to_status)?;

        if req.follow {
            let mut stream = Box::pin(loop_stream(file, last_position));
            tokio::spawn(async move {
                while let Some(content) = stream.next().await {
                    match tx
                        .send(Result::<_, Status>::Ok(LogResponse {
                            content: content.unwrap(),
                        }))
                        .await
                    {
                        Ok(_) => {
                            // item (server response) was queued to be send to client
                        }
                        Err(_) => {
                            // output_stream was build from rx and both are dropped
                            break;
                        }
                    }
                }
            });
        }

        let output_stream = ReceiverStream::new(rx);
        Ok(Response::new(Box::pin(output_stream) as Self::LogsStream))
    }
}

fn loop_stream(mut file: File, mut last_position: usize) -> impl Stream<Item = Result<String>> {
    let mut contents = String::new();
    let mut interval = time::interval(Duration::from_millis(200));

    try_stream! {
        loop {
            contents.truncate(0);
            file.seek(SeekFrom::Start(last_position as u64)).await?;
            last_position += file.read_to_string(&mut contents).await?;
            if !contents.is_empty() {
                yield contents.clone();
            }

            interval.tick().await;
        }
    }
}
