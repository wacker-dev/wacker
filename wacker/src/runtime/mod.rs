mod cli;
mod host;
mod http;
mod logs;

use crate::{PROGRAM_TYPE_CLI, PROGRAM_TYPE_HTTP};
use ahash::AHashMap;
use anyhow::Result;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::fs;
use std::fs::File;
use std::sync::Arc;
use wasmtime::Config;

#[derive(Clone, Default, Serialize, Deserialize)]
pub struct ProgramMeta {
    pub path: String,
    pub program_type: u32,
    pub addr: Option<String>,
    pub args: Vec<String>,
}

#[async_trait]
pub trait Engine: Send + Sync + 'static {
    async fn run(&self, meta: ProgramMeta, stdout: File) -> Result<()>;
}

pub fn new_engines() -> Result<AHashMap<u32, Arc<dyn Engine>>> {
    let config = default_wasmtime_config()?;
    let cli_engine: Arc<dyn Engine> = Arc::new(cli::CliEngine::new(&config)?);
    let http_engine: Arc<dyn Engine> = Arc::new(http::HttpEngine::new(&config)?);

    Ok(AHashMap::from([
        (PROGRAM_TYPE_CLI, cli_engine),
        (PROGRAM_TYPE_HTTP, http_engine),
    ]))
}

fn default_wasmtime_config() -> Result<Config> {
    let mut config = Config::new();
    // We need this engine's `Store`s to be async, and consume fuel, so
    // that they can co-operatively yield during execution.
    config.async_support(true);
    config.consume_fuel(true);
    config.cache_config_load_default()?;
    config.cranelift_opt_level(wasmtime::OptLevel::SpeedAndSize);
    config.wasm_component_model(true);
    Ok(config)
}

async fn read(path: &str) -> Result<Vec<u8>> {
    match path.starts_with("http") {
        true => {
            let bytes = reqwest::get(path).await?.bytes().await?;
            Ok(Vec::from(bytes))
        }
        false => Ok(fs::read(path)?),
    }
}
