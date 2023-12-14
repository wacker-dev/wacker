use anyhow::{anyhow, Result};
use clap::Parser;
use tonic::transport::Channel;
use wacker_api::{modules_client::ModulesClient, DeleteRequest};

#[derive(Parser)]
pub struct DeleteCommand {
    /// Module ID
    #[arg(required = true)]
    id: String,
}

impl DeleteCommand {
    pub async fn execute(self, mut client: ModulesClient<Channel>) -> Result<()> {
        match client.delete(DeleteRequest { id: self.id }).await {
            Ok(_) => Ok(()),
            Err(err) => Err(anyhow!(err.message().to_string())),
        }
    }
}
