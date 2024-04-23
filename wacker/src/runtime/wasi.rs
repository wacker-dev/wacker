use crate::runtime::{
    logs::LogStream,
    {Engine, ProgramMeta},
};
use anyhow::{anyhow, Result};
use async_trait::async_trait;
use std::fs::{read, File};
use std::path::Path;
use wasi_common::{tokio, I32Exit};
use wasmtime::component::{Component, ResourceTable};
use wasmtime::{Module, Store};
use wasmtime_wasi::bindings::Command;
use wasmtime_wasi::{WasiCtx, WasiCtxBuilder, WasiView};

#[derive(Clone)]
pub struct WasiEngine {
    engine: wasmtime::Engine,
}

struct Host {
    ctx: WasiCtx,
    table: ResourceTable,
}

impl WasiView for Host {
    fn table(&mut self) -> &mut ResourceTable {
        &mut self.table
    }
    fn ctx(&mut self) -> &mut WasiCtx {
        &mut self.ctx
    }
}

enum RunTarget {
    Core(Module),
    Component(Component),
}

impl WasiEngine {
    pub fn new(engine: wasmtime::Engine) -> Self {
        Self { engine }
    }

    fn load_module_contents(&self, engine: &wasmtime::Engine, path: &Path) -> Result<RunTarget> {
        let bytes = read(path)?;
        let mut builder = wasmtime::CodeBuilder::new(engine);
        let wasm_builder = builder.wasm(&bytes, Some(path))?;
        match wasmparser::Parser::is_component(&bytes) {
            true => Ok(RunTarget::Component(wasm_builder.compile_component()?)),
            false => Ok(RunTarget::Core(wasm_builder.compile_module()?)),
        }
    }
}

#[async_trait]
impl Engine for WasiEngine {
    async fn run(&self, meta: ProgramMeta, stdout: File) -> Result<()> {
        let mut args = meta.args;
        args.insert(0, meta.path.clone());

        match self.load_module_contents(&self.engine, Path::new(&meta.path))? {
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
                        ctx,
                        table: ResourceTable::new(),
                    },
                );
                store.set_fuel(u64::MAX)?;
                store.fuel_async_yield_interval(Some(10000))?;

                let mut linker = wasmtime::component::Linker::new(&self.engine);
                wasmtime_wasi::add_to_linker_async(&mut linker)?;

                let (command, _instance) = Command::instantiate_async(&mut store, &component, &linker).await?;
                match command.wasi_cli_run().call_run(&mut store).await {
                    Ok(_) => Ok(()),
                    Err(_) => Err(anyhow!("call run function error")),
                }
            }
        }
    }
}
