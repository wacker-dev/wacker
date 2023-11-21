use anyhow::Result;
use clap::Parser;
use tonic::transport::Channel;
use wacker_api::{modules_client::ModulesClient, RestartRequest};

#[derive(Parser, PartialEq)]
#[structopt(name = "restart")]
pub struct RestartCommand {
    /// Module ID
    #[arg(required = true)]
    id: String,
}

impl RestartCommand {
    pub async fn execute(self, channel: Channel) -> Result<()> {
        let mut client = ModulesClient::new(channel);

        let request = tonic::Request::new(RestartRequest { id: self.id });
        client.restart(request).await?;

        Ok(())
    }
}
