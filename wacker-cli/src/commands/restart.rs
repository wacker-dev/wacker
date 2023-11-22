use anyhow::{anyhow, Result};
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
        match client.restart(request).await {
            Ok(_) => Ok(()),
            Err(err) => Err(anyhow!(err.message().to_string())),
        }
    }
}
