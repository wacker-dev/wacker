use crate::runtime::{
    host::Host,
    logs::LogStream,
    read, {Engine, ProgramMeta},
};
use anyhow::{anyhow, Result};
use async_trait::async_trait;
use std::fs::File;
use wasi_common::{tokio, I32Exit};
use wasmtime::component::{Component, ResourceTable};
use wasmtime::{Config, Module, Store};
use wasmtime_wasi::{bindings::Command, WasiCtxBuilder};
use wasmtime_wasi_http::WasiHttpCtx;

#[derive(Clone)]
pub struct CliEngine {
    engine: wasmtime::Engine,
}

enum RunTarget {
    Core(Module),
    Component(Component),
}

impl CliEngine {
    pub fn new(config: &Config) -> Result<Self> {
        Ok(Self {
            engine: wasmtime::Engine::new(config)?,
        })
    }

    async fn load_module_contents(&self, engine: &wasmtime::Engine, path: &str) -> Result<RunTarget> {
        let bytes = read(path).await?;
        let mut builder = wasmtime::CodeBuilder::new(engine);
        let wasm_builder = builder.wasm(&bytes, Some(path.as_ref()))?;
        match wasmparser::Parser::is_component(&bytes) {
            true => Ok(RunTarget::Component(wasm_builder.compile_component()?)),
            false => Ok(RunTarget::Core(wasm_builder.compile_module()?)),
        }
    }
}

#[async_trait]
impl Engine for CliEngine {
    async fn run(&self, meta: ProgramMeta, stdout: File) -> Result<()> {
        let mut args = meta.args;
        args.insert(0, meta.path.clone());

        match self.load_module_contents(&self.engine, &meta.path).await? {
            RunTarget::Core(module) => {
                let stderr = stdout.try_clone()?;

                let wasi_stdout = cap_std::fs::File::from_std(stdout);
                let wasi_stdout = tokio::File::from_cap_std(wasi_stdout);
                let wasi_stderr = cap_std::fs::File::from_std(stderr);
                let wasi_stderr = tokio::File::from_cap_std(wasi_stderr);

                let wasi = tokio::WasiCtxBuilder::new()
                    .inherit_stdin()
                    .stdout(Box::new(wasi_stdout))
                    .stderr(Box::new(wasi_stderr))
                    .args(args.as_ref())?
                    .inherit_env()?
                    .build();
                let mut store = Store::new(&self.engine, wasi);
                store.set_fuel(u64::MAX)?;
                store.fuel_async_yield_interval(Some(10000))?;

                let mut linker = wasmtime::Linker::new(&self.engine);
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
            RunTarget::Component(component) => {
                let stderr = stdout.try_clone()?;

                let ctx = WasiCtxBuilder::new()
                    .inherit_stdin()
                    .stdout(LogStream { output: stdout })
                    .stderr(LogStream { output: stderr })
                    .args(args.as_ref())
                    .inherit_env()
                    .build();
                let mut store = Store::new(
                    &self.engine,
                    Host {
                        table: ResourceTable::new(),
                        ctx,
                        http: WasiHttpCtx::new(),
                    },
                );
                store.set_fuel(u64::MAX)?;
                store.fuel_async_yield_interval(Some(10000))?;

                let mut linker = wasmtime::component::Linker::new(&self.engine);
                wasmtime_wasi::add_to_linker_async(&mut linker)?;
                wasmtime_wasi_http::proxy::add_only_http_to_linker(&mut linker)?;

                let (command, _instance) = Command::instantiate_async(&mut store, &component, &linker).await?;
                match command.wasi_cli_run().call_run(&mut store).await {
                    Ok(_) => Ok(()),
                    Err(e) => Err(anyhow!("call run function error: {}", e)),
                }
            }
        }
    }
}
