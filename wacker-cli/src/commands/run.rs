use anyhow::Result;
use clap::Parser;
use std::path::Path;
use tonic::transport::Channel;
use wacker_api::{modules_client::ModulesClient, RunRequest};

#[derive(Parser, PartialEq)]
#[structopt(name = "run")]
pub struct RunCommand {
    /// Module file path
    #[arg(required = true)]
    path: String,
}

impl RunCommand {
    /// Executes the command.
    pub async fn execute(self, channel: Channel) -> Result<()> {
        let mut client = ModulesClient::new(channel);

        let path = Path::new(self.path.as_str());
        let request = tonic::Request::new(RunRequest {
            name: path.file_name().unwrap().to_str().unwrap().to_string(),
            path: path.to_str().unwrap().to_string(),
        });

        client.run(request).await?;

        Ok(())
    }
}
