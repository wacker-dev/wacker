use anyhow::{anyhow, Result};
use clap::Parser;
use tonic::transport::Channel;
use wacker::{StopRequest, WackerClient};

#[derive(Parser)]
pub struct StopCommand {
    /// Program ID
    #[arg(required = true)]
    id: String,
}

impl StopCommand {
    /// Executes the command.
    pub async fn execute(self, mut client: WackerClient<Channel>) -> Result<()> {
        match client.stop(StopRequest { id: self.id }).await {
            Ok(_) => Ok(()),
            Err(err) => Err(anyhow!(err.message().to_string())),
        }
    }
}
