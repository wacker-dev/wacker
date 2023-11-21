use crate::run::{run_module, Environment};
use anyhow::{bail, Error, Result};
use log::{error, info, warn};
use rand::Rng;
use std::collections::HashMap;
use std::fs::{create_dir, OpenOptions};
use std::io::{ErrorKind, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use tokio::{
    sync::{oneshot, oneshot::error::TryRecvError},
    task,
};
use tonic::{IntoRequest, Request, Response, Status};

pub struct Service {
    env: Environment,
    modules: Arc<Mutex<HashMap<String, InnerModule>>>,
    home_dir: PathBuf,
}

struct InnerModule {
    path: String,
    receiver: oneshot::Receiver<Error>,
    handler: task::JoinHandle<()>,
    status: wacker_api::ModuleStatus,
    error: Option<Error>,
}

impl Service {
    pub fn new(home_dir: PathBuf) -> Result<Self, Error> {
        if let Err(e) = create_dir(home_dir.join(".wacker/logs")) {
            if e.kind() != ErrorKind::AlreadyExists {
                bail!("create logs dir failed: {}", e)
            }
        }

        // Create an environment shared by all wasm execution. This contains
        // the `Engine` we are executing.
        let env = Environment::new()?;
        let modules = HashMap::new();
        Ok(Self {
            env,
            modules: Arc::new(Mutex::new(modules)),
            home_dir,
        })
    }

    fn stop_and_remove(&self, id: &str) -> Option<String> {
        let mut modules = self.modules.lock().unwrap();
        match modules.get(id) {
            Some(module) => {
                let path = module.path.clone();
                if !module.handler.is_finished() {
                    module.handler.abort();
                }
                modules.remove(id);
                Option::from(path)
            }
            None => None,
        }
    }
}

fn generate_random_string(length: usize) -> String {
    const CHARSET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789";
    let mut rng = rand::thread_rng();

    (0..length)
        .map(|_| {
            let index = rng.gen_range(0..CHARSET.len());
            CHARSET[index] as char
        })
        .collect()
}

#[tonic::async_trait]
impl wacker_api::modules_server::Modules for Service {
    async fn run(&self, request: Request<wacker_api::RunRequest>) -> Result<Response<()>, Status> {
        let req = request.into_inner();

        let file_path = Path::new(&req.path);
        let name = file_path.file_stem();
        if name.is_none() {
            return Err(Status::internal(format!(
                "failed to get file name in path {}",
                req.path
            )));
        }
        let id = format!(
            "{}-{}",
            name.unwrap().to_str().unwrap(),
            generate_random_string(7)
        );

        info!("Execute newly added module: {} ({})", id, req.path);

        let mut modules = self.modules.lock().unwrap();
        let (sender, receiver) = oneshot::channel();
        let env = self.env.clone();

        let mut stdout = OpenOptions::new()
            .create(true)
            .append(true)
            .open(self.home_dir.join(".wacker/logs").join(id.clone()))?;
        let stdout_clone = stdout.try_clone()?;

        modules.insert(
            id.clone(),
            InnerModule {
                path: req.path.clone(),
                receiver,
                handler: task::spawn(async move {
                    match run_module(env, &req.path, stdout_clone).await {
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
                status: wacker_api::ModuleStatus::Running,
                error: None,
            },
        );
        Ok(Response::new(()))
    }

    async fn list(&self, _: Request<()>) -> Result<Response<wacker_api::ListResponse>, Status> {
        let mut reply = wacker_api::ListResponse { modules: vec![] };
        let mut modules = self.modules.lock().unwrap();

        for (id, inner) in modules.iter_mut() {
            match inner.status {
                wacker_api::ModuleStatus::Running if inner.handler.is_finished() => {
                    inner.status = match inner.receiver.try_recv() {
                        Ok(err) => {
                            inner.error = Option::from(err);
                            wacker_api::ModuleStatus::Error
                        }
                        Err(TryRecvError::Empty) | Err(TryRecvError::Closed) => {
                            wacker_api::ModuleStatus::Finished
                        }
                    };
                }
                _ => {}
            };

            reply.modules.push(wacker_api::Module {
                id: id.clone(),
                path: inner.path.clone(),
                status: i32::from(inner.status),
            });
        }

        Ok(Response::new(reply))
    }

    async fn stop(
        &self,
        request: Request<wacker_api::StopRequest>,
    ) -> Result<Response<()>, Status> {
        let req = request.into_inner();

        let mut modules = self.modules.lock().unwrap();
        match modules.get_mut(&req.id) {
            Some(module) => {
                info!("Stop the module: {}", req.id);
                if !module.handler.is_finished() {
                    module.handler.abort();
                    module.status = wacker_api::ModuleStatus::Stopped;
                }
                Ok(Response::new(()))
            }
            None => Err(Status::not_found(format!("module {} not exists", req.id))),
        }
    }

    async fn restart(
        &self,
        request: Request<wacker_api::RestartRequest>,
    ) -> Result<Response<()>, Status> {
        let req = request.into_inner();
        info!("Restart the module: {}", req.id);

        match self.stop_and_remove(&req.id) {
            Some(path) => {
                self.run(wacker_api::RunRequest { path }.into_request())
                    .await
            }
            None => Err(Status::not_found(format!("module {} not exists", req.id))),
        }
    }

    async fn delete(
        &self,
        request: Request<wacker_api::DeleteRequest>,
    ) -> Result<Response<()>, Status> {
        let req = request.into_inner();
        info!("Delete the module: {}", req.id);

        self.stop_and_remove(&req.id);
        Ok(Response::new(()))
    }
}
