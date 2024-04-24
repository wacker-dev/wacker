use anyhow::{anyhow, Result};
use clap::Parser;
use tonic::transport::Channel;
use wacker::{Client, StopRequest};

#[derive(Parser)]
pub struct StopCommand {
    /// Program IDs
    #[arg(required = true, value_name = "IDs")]
    ids: Vec<String>,
}

impl StopCommand {
    /// Executes the command.
    pub async fn execute(self, mut client: Client<Channel>) -> Result<()> {
        match client.stop(StopRequest { ids: self.ids }).await {
            Ok(_) => Ok(()),
            Err(err) => Err(anyhow!(err.message().to_string())),
        }
    }
}
