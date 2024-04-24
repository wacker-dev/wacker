use anyhow::{bail, Result};
use clap::Parser;
use std::io::{stdout, Write};
use tokio_stream::StreamExt;
use tonic::transport::Channel;
use wacker::{Client, LogRequest};

#[derive(Parser)]
pub struct LogsCommand {
    /// Program ID
    #[arg(required = true)]
    id: String,

    /// Follow log output
    #[arg(short, long)]
    follow: bool,

    /// Number of lines to show from the end of the logs
    #[arg(short = 'n', long, value_name = "n")]
    tail: Option<u32>,
}

impl LogsCommand {
    /// Executes the command.
    pub async fn execute(self, mut client: Client<Channel>) -> Result<()> {
        match client
            .logs(LogRequest {
                id: self.id,
                follow: self.follow,
                tail: self.tail.unwrap_or(0),
            })
            .await
        {
            Ok(resp) => {
                let mut resp = resp.into_inner();
                while let Some(item) = resp.next().await {
                    print!("{}", item.unwrap().content);
                    stdout().flush()?;
                }
                Ok(())
            }
            Err(err) => bail!(err.message().to_string()),
        }
    }
}
