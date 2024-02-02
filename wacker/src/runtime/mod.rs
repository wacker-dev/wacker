mod http;
mod wasi;

pub use self::http::HttpEngine;
pub use self::wasi::WasiEngine;

use anyhow::Result;
use wasmtime::{Config, Engine, OptLevel};

pub fn new_engine() -> Result<Engine> {
    let mut config = Config::new();
    // We need this engine's `Store`s to be async, and consume fuel, so
    // that they can co-operatively yield during execution.
    config.async_support(true);
    config.consume_fuel(true);
    config.cache_config_load_default()?;
    config.cranelift_opt_level(OptLevel::SpeedAndSize);
    config.wasm_component_model(true);

    // Initialize global per-process state. This state will be shared amongst all
    // threads. Notably this includes the compiled module as well as a `Linker`,
    // which contains all our host functions we want to define.
    Engine::new(&config)
}
