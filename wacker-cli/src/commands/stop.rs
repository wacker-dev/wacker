use anyhow::Result;
use clap::Parser;
use tonic::transport::Channel;
use wacker_api::{modules_client::ModulesClient, StopRequest};

#[derive(Parser, PartialEq)]
#[structopt(name = "stop")]
pub struct StopCommand {
    /// Module ID
    #[arg(required = true)]
    id: String,
}

impl StopCommand {
    /// Executes the command.
    pub async fn execute(self, channel: Channel) -> Result<()> {
        let mut client = ModulesClient::new(channel);

        let request = tonic::Request::new(StopRequest { id: self.id });
        client.stop(request).await?;

        Ok(())
    }
}
