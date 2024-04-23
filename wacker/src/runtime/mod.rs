mod http;
mod logs;
mod wasi;

use ahash::AHashMap;
use anyhow::Result;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::fs::File;
use std::sync::Arc;

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

pub const PROGRAM_TYPE_WASI: u32 = 0;
pub const PROGRAM_TYPE_HTTP: u32 = 1;

pub fn new_engines() -> Result<AHashMap<u32, Arc<dyn Engine>>> {
    let wasmtime_engine = new_wasmtime_engine()?;
    let wasi_engine: Arc<dyn Engine> = Arc::new(wasi::WasiEngine::new(wasmtime_engine.clone()));
    let http_engine: Arc<dyn Engine> = Arc::new(http::HttpEngine::new(wasmtime_engine));

    Ok(AHashMap::from([
        (PROGRAM_TYPE_WASI, wasi_engine),
        (PROGRAM_TYPE_HTTP, http_engine),
    ]))
}

fn new_wasmtime_engine() -> Result<wasmtime::Engine> {
    let mut config = wasmtime::Config::new();
    // We need this engine's `Store`s to be async, and consume fuel, so
    // that they can co-operatively yield during execution.
    config.async_support(true);
    config.consume_fuel(true);
    config.cache_config_load_default()?;
    config.cranelift_opt_level(wasmtime::OptLevel::SpeedAndSize);
    config.wasm_component_model(true);

    // Initialize global per-process state. This state will be shared amongst all
    // threads. Notably this includes the compiled module as well as a `Linker`,
    // which contains all our host functions we want to define.
    wasmtime::Engine::new(&config)
}
