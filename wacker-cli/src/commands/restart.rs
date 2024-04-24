use anyhow::{anyhow, Result};
use clap::Parser;
use tonic::transport::Channel;
use wacker::{Client, RestartRequest};

#[derive(Parser)]
pub struct RestartCommand {
    /// Program IDs
    #[arg(required = true, value_name = "IDs")]
    ids: Vec<String>,
}

impl RestartCommand {
    pub async fn execute(self, mut client: Client<Channel>) -> Result<()> {
        match client.restart(RestartRequest { ids: self.ids }).await {
            Ok(_) => Ok(()),
            Err(err) => Err(anyhow!(err.message().to_string())),
        }
    }
}
