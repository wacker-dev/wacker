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
    receiver: oneshot::Receiver<Error>,
    handler: task::JoinHandle<()>,
    status: Option<wacker_api::ModuleStatus>,
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
                status: None,
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
                Some(status) => reply.modules.push(wacker_api::Module {
                    name: name.clone(),
                    status: i32::from(status),
                }),
                None => {
                    let status = if inner.handler.is_finished() {
                        match inner.receiver.try_recv() {
                            Ok(err) => {
                                inner.error = Option::from(err);
                                wacker_api::ModuleStatus::Error
                            }
                            Err(TryRecvError::Empty) | Err(TryRecvError::Closed) => {
                                wacker_api::ModuleStatus::Finished
                            }
                        }
                    } else {
                        wacker_api::ModuleStatus::Running
                    };
                    inner.status = Option::from(status);
                    reply.modules.push(wacker_api::Module {
                        name: name.clone(),
                        status: i32::from(status),
                    });
                }
            };
        }

        Ok(Response::new(reply))
    }
}
