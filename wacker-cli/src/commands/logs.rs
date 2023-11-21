use anyhow::{bail, Result};
use clap::Parser;
use std::process::Command;
use tonic::transport::Channel;

#[derive(Parser, PartialEq)]
#[structopt(name = "logs", aliases = &["log"])]
pub struct LogsCommand {
    /// Module ID
    #[arg(required = true)]
    id: String,

    /// Follow log output
    #[arg(short, long)]
    follow: bool,

    /// Number of lines to show from the end of the logs
    #[arg(short, long, value_name = "n")]
    tail: Option<u32>,
}

impl LogsCommand {
    /// Executes the command.
    pub async fn execute(self, _: Channel) -> Result<()> {
        let home_dir = dirs::home_dir().expect("Can't get home dir");
        let path = home_dir.join(".wacker/logs").join(self.id);
        let mut tail_args = vec![];
        if self.follow {
            tail_args.push("-f".to_string());
        }
        if self.tail.is_some() {
            tail_args.push(format!("-n {}", self.tail.unwrap()));
        }
        tail_args.push(path.display().to_string());

        match Command::new("tail").args(tail_args).spawn() {
            Ok(mut child) => {
                let status = child.wait().expect("Failed to wait for child process");
                if !status.success() {
                    bail!("tail command failed with: {:?}", status)
                }
                Ok(())
            }
            Err(err) => {
                bail!("Error executing tail command: {}", err)
            }
        }
    }
}
