use crate::run::{run_module, Environment};
use anyhow::{Error, Result};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tokio::task;
use tonic::{Request, Response, Status};
use wacker_api::{module_server, RunRequest};

pub struct Module {
    env: Environment,
    modules: Arc<Mutex<HashMap<String, task::JoinHandle<Result<()>>>>>,
}

impl Module {
    pub fn new() -> Result<Self, Error> {
        // Create an environment shared by all wasm execution. This contains
        // the `Engine` we are executing.
        let env = Environment::new()?;
        let modules: HashMap<String, task::JoinHandle<Result<()>>> = HashMap::new();
        Ok(Self {
            env,
            modules: Arc::new(Mutex::new(modules)),
        })
    }
}

#[tonic::async_trait]
impl module_server::Module for Module {
    async fn run(&self, request: Request<RunRequest>) -> Result<Response<()>, Status> {
        let req = request.into_inner();

        let mut modules = self.modules.lock().unwrap();

        if modules.contains_key(&req.name) {
            return Ok(Response::new(()));
        }

        let env = self.env.clone();
        modules.insert(
            req.name,
            task::spawn(async move { run_module(env, &req.path).await }),
        );
        Ok(Response::new(()))
    }
}
