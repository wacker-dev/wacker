use anyhow::{anyhow, Result};
use clap::Parser;
use tonic::transport::Channel;
use wacker::{RunRequest, WackerClient};

#[derive(Parser)]
pub struct RunCommand {
    /// Program file path
    #[arg(required = true)]
    path: String,
    /// Arguments to pass to the WebAssembly module.
    #[arg(trailing_var_arg = true)]
    args: Vec<String>,
}

impl RunCommand {
    /// Executes the command.
    pub async fn execute(self, mut client: WackerClient<Channel>) -> Result<()> {
        match client
            .run(RunRequest {
                path: self.path.to_string(),
                args: self.args,
            })
            .await
        {
            Ok(_) => Ok(()),
            Err(err) => Err(anyhow!(err.message().to_string())),
        }
    }
}
