use anyhow::Result;
use clap::Parser;
use tonic::transport::Channel;
use wacker_api::{modules_client::ModulesClient, RestartRequest};

#[derive(Parser, PartialEq)]
#[structopt(name = "restart")]
pub struct RestartCommand {
    #[arg(required = true)]
    name: String,
}

impl RestartCommand {
    pub async fn execute(self, channel: Channel) -> Result<()> {
        let mut client = ModulesClient::new(channel);

        let request = tonic::Request::new(RestartRequest { name: self.name });
        client.restart(request).await?;

        Ok(())
    }
}
