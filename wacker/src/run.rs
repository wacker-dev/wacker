use anyhow::{anyhow, Error, Result};
use std::sync::Arc;
use wasi_common::I32Exit;
use wasmtime::{Config, Engine, Linker, Module, OptLevel, Store};
use wasmtime_wasi::{tokio::WasiCtxBuilder, WasiCtx};

#[derive(Clone)]
pub struct Environment {
    engine: Engine,
    linker: Arc<Linker<WasiCtx>>,
}

impl Environment {
    pub fn new() -> Result<Self, Error> {
        let mut config = Config::new();
        // We need this engine's `Store`s to be async, and consume fuel, so
        // that they can co-operatively yield during execution.
        config.async_support(true);
        config.consume_fuel(true);
        config.cache_config_load_default()?;
        config.cranelift_opt_level(OptLevel::SpeedAndSize);

        // Initialize global per-process state. This state will be shared amongst all
        // threads. Notably this includes the compiled module as well as a `Linker`,
        // which contains all our host functions we want to define.
        let engine = Engine::new(&config)?;

        // A `Linker` is shared in the environment amongst all stores, and this
        // linker is used to instantiate the `module` above. This example only
        // adds WASI functions to the linker, notably the async versions built
        // on tokio.
        let mut linker = Linker::new(&engine);
        wasmtime_wasi::tokio::add_to_linker(&mut linker, |cx| cx)?;

        Ok(Self {
            engine,
            linker: Arc::new(linker),
        })
    }
}

pub async fn run_module(env: Environment, path: &str) -> Result<()> {
    // Create a WASI context and put it in a Store; all instances in the store
    // share this context. `WasiCtxBuilder` provides a number of ways to
    // configure what the target program will have access to.
    let wasi = WasiCtxBuilder::new()
        .inherit_stdio()
        .inherit_args()?
        .build();
    let mut store = Store::new(&env.engine, wasi);

    // WebAssembly execution will be paused for an async yield every time it
    // consumes 10000 fuel. Fuel will be refilled u64::MAX times.
    store.out_of_fuel_async_yield(u64::MAX, 10000);

    // Instantiate our module with the imports we've created, and run it.
    let module = Module::from_file(&env.engine, path)?;

    // Instantiate into our own unique store using the shared linker, afterwards
    // acquiring the `_start` function for the module and executing it.
    let instance = env.linker.instantiate_async(&mut store, &module).await?;
    let func = instance
        .get_func(&mut store, "_start")
        .or_else(|| instance.get_func(&mut store, ""));

    match func {
        Some(func) => match func.call_async(&mut store, &[], &mut []).await {
            Ok(()) => Ok(()),
            Err(err) => {
                match err.downcast_ref::<I32Exit>() {
                    // Ignore errors with exit code 0
                    Some(exit_error) if exit_error.0 == 0 => Ok(()),
                    _ => Err(err),
                }
            }
        },
        None => Err(anyhow!("no main function to run")),
    }
}
