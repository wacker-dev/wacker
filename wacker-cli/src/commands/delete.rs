use anyhow::Result;
use clap::Parser;
use tonic::transport::Channel;
use wacker_api::{modules_client::ModulesClient, DeleteRequest};

#[derive(Parser, PartialEq)]
#[structopt(name = "delete", aliases = &["rm"])]
pub struct DeleteCommand {
    #[arg(required = true)]
    name: String,
}

impl DeleteCommand {
    pub async fn execute(self, channel: Channel) -> Result<()> {
        let mut client = ModulesClient::new(channel);

        let request = tonic::Request::new(DeleteRequest { name: self.name });
        client.delete(request).await?;

        Ok(())
    }
}
