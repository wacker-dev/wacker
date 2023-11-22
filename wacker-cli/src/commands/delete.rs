use anyhow::{anyhow, Result};
use clap::Parser;
use tonic::transport::Channel;
use wacker_api::{modules_client::ModulesClient, DeleteRequest};

#[derive(Parser, PartialEq)]
#[structopt(name = "delete", aliases = &["rm"])]
pub struct DeleteCommand {
    /// Module ID
    #[arg(required = true)]
    id: String,
}

impl DeleteCommand {
    pub async fn execute(self, channel: Channel) -> Result<()> {
        let mut client = ModulesClient::new(channel);
        let request = tonic::Request::new(DeleteRequest { id: self.id });
        match client.delete(request).await {
            Ok(_) => Ok(()),
            Err(err) => Err(anyhow!(err.message().to_string())),
        }
    }
}
