use anyhow::{anyhow, Result};
use clap::Parser;
use tonic::transport::Channel;
use wacker::{RestartRequest, WackerClient};

#[derive(Parser)]
pub struct RestartCommand {
    /// Program ID
    #[arg(required = true)]
    id: String,
}

impl RestartCommand {
    pub async fn execute(self, mut client: WackerClient<Channel>) -> Result<()> {
        match client.restart(RestartRequest { id: self.id }).await {
            Ok(_) => Ok(()),
            Err(err) => Err(anyhow!(err.message().to_string())),
        }
    }
}
