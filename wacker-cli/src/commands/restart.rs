use anyhow::{anyhow, Result};
use clap::Parser;
use tonic::transport::Channel;
use wacker_api::{modules_client::ModulesClient, RestartRequest};

#[derive(Parser)]
pub struct RestartCommand {
    /// Module ID
    #[arg(required = true)]
    id: String,
}

impl RestartCommand {
    pub async fn execute(self, mut client: ModulesClient<Channel>) -> Result<()> {
        match client.restart(RestartRequest { id: self.id }).await {
            Ok(_) => Ok(()),
            Err(err) => Err(anyhow!(err.message().to_string())),
        }
    }
}
