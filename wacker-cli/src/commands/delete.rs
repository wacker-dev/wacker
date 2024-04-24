use anyhow::{anyhow, Result};
use clap::Parser;
use tonic::transport::Channel;
use wacker::{Client, DeleteRequest};

#[derive(Parser)]
pub struct DeleteCommand {
    /// Program ID
    #[arg(required = true)]
    id: String,
}

impl DeleteCommand {
    pub async fn execute(self, mut client: Client<Channel>) -> Result<()> {
        match client.delete(DeleteRequest { id: self.id }).await {
            Ok(_) => Ok(()),
            Err(err) => Err(anyhow!(err.message().to_string())),
        }
    }
}
