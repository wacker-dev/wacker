use anyhow::{anyhow, Result};
use clap::Parser;
use tonic::transport::Channel;
use wacker_api::{modules_client::ModulesClient, StopRequest};

#[derive(Parser)]
pub struct StopCommand {
    /// Module ID
    #[arg(required = true)]
    id: String,
}

impl StopCommand {
    /// Executes the command.
    pub async fn execute(self, mut client: ModulesClient<Channel>) -> Result<()> {
        match client.stop(StopRequest { id: self.id }).await {
            Ok(_) => Ok(()),
            Err(err) => Err(anyhow!(err.message().to_string())),
        }
    }
}
