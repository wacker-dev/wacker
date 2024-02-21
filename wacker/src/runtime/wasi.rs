use anyhow::anyhow;
use std::fs::File;
use wasi_common::{tokio, I32Exit};
use wasmtime::{Engine, Linker, Module, Store};

#[derive(Clone)]
pub struct WasiEngine {
    engine: Engine,
}

impl WasiEngine {
    pub fn new(engine: Engine) -> Self {
        Self { engine }
    }

    pub async fn run_wasi(&self, path: &str, stdout: File) -> anyhow::Result<()> {
        let stderr = stdout.try_clone()?;

        let wasi_stdout = cap_std::fs::File::from_std(stdout);
        let wasi_stdout = tokio::File::from_cap_std(wasi_stdout);
        let wasi_stderr = cap_std::fs::File::from_std(stderr);
        let wasi_stderr = tokio::File::from_cap_std(wasi_stderr);

        // Create a WASI context and put it in a Store; all instances in the store
        // share this context. `WasiCtxBuilder` provides a number of ways to
        // configure what the target program will have access to.
        let wasi = tokio::WasiCtxBuilder::new()
            .inherit_stdin()
            .stdout(Box::new(wasi_stdout))
            .stderr(Box::new(wasi_stderr))
            .inherit_args()?
            .build();
        let mut store = Store::new(&self.engine, wasi);

        // Put effectively unlimited fuel so it can run forever.
        store.set_fuel(u64::MAX)?;
        // WebAssembly execution will be paused for an async yield every time it
        // consumes 10000 fuel.
        store.fuel_async_yield_interval(Some(10000))?;

        // Instantiate our module with the imports we've created, and run it.
        let module = Module::from_file(&self.engine, path)?;

        // A `Linker` is shared in the environment amongst all stores, and this
        // linker is used to instantiate the `module` above. This example only
        // adds WASI functions to the linker, notably the async versions built
        // on tokio.
        let mut linker = Linker::new(&self.engine);
        tokio::add_to_linker(&mut linker, |cx| cx)?;

        // Instantiate into our own unique store using the shared linker, afterwards
        // acquiring the `_start` function for the module and executing it.
        let instance = linker.instantiate_async(&mut store, &module).await?;
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
}
