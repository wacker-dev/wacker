use anyhow::{anyhow, Result};
use clap::Parser;
use tonic::transport::Channel;
use wacker::{RunRequest, WackerClient};

#[derive(Parser)]
pub struct RunCommand {
    /// Program file path
    #[arg(required = true)]
    path: String,
}

impl RunCommand {
    /// Executes the command.
    pub async fn execute(self, mut client: WackerClient<Channel>) -> Result<()> {
        match client
            .run(RunRequest {
                path: self.path.to_string(),
            })
            .await
        {
            Ok(_) => Ok(()),
            Err(err) => Err(anyhow!(err.message().to_string())),
        }
    }
}
