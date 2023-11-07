use anyhow::Result;
use clap::Parser;
use tonic::transport::Channel;
use wacker_api::{modules_client::ModulesClient, StopRequest};

#[derive(Parser, PartialEq)]
#[structopt(name = "stop")]
pub struct StopCommand {
    #[arg(required = true)]
    name: String,
}

impl StopCommand {
    /// Executes the command.
    pub async fn execute(self, channel: Channel) -> Result<()> {
        let mut client = ModulesClient::new(channel);

        let request = tonic::Request::new(StopRequest { name: self.name });
        client.stop(request).await?;

        Ok(())
    }
}
