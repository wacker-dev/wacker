use crate::run::{run_module, Environment};
use anyhow::{Error, Result};
use log::{info, warn};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tokio::sync::{oneshot, oneshot::error::TryRecvError};
use tokio::task;
use tonic::{Request, Response, Status};
use wacker_api;

pub struct Service {
    env: Environment,
    modules: Arc<Mutex<HashMap<String, InnerModule>>>,
}

struct InnerModule {
    path: String,
    receiver: oneshot::Receiver<Error>,
    handler: task::JoinHandle<()>,
    status: wacker_api::ModuleStatus,
    error: Option<Error>,
}

impl Service {
    pub fn new() -> Result<Self, Error> {
        // Create an environment shared by all wasm execution. This contains
        // the `Engine` we are executing.
        let env = Environment::new()?;
        let modules = HashMap::new();
        Ok(Self {
            env,
            modules: Arc::new(Mutex::new(modules)),
        })
    }
}

#[tonic::async_trait]
impl wacker_api::modules_server::Modules for Service {
    async fn run(&self, request: Request<wacker_api::RunRequest>) -> Result<Response<()>, Status> {
        let req = request.into_inner();

        let mut modules = self.modules.lock().unwrap();

        if modules.contains_key(&req.name) {
            return Ok(Response::new(()));
        }

        info!("Execute newly added module: {} ({})", req.name, req.path);
        let (sender, receiver) = oneshot::channel();
        let env = self.env.clone();
        modules.insert(
            req.name,
            InnerModule {
                path: req.path.clone(),
                receiver,
                handler: task::spawn(async move {
                    match run_module(env, &req.path).await {
                        Ok(_) => {}
                        Err(e) => {
                            if let Err(_) = sender.send(e) {
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

        for (name, inner) in modules.iter_mut() {
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
                name: name.clone(),
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
        match modules.get_mut(&req.name) {
            Some(module) => {
                info!("Stop the module: {}", req.name);
                module.handler.abort();
                module.status = wacker_api::ModuleStatus::Stopped;
                Ok(Response::new(()))
            }
            None => Err(Status::not_found(format!("module {} not exists", req.name))),
        }
    }
}
