use anyhow::{bail, Result};
use clap::Parser;
use std::process::Command;
use wacker::Config;

#[derive(Parser)]
pub struct LogsCommand {
    /// Module ID
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
    pub async fn execute(self) -> Result<()> {
        let config = Config::new()?;
        let path = config.logs_dir.join(self.id);
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
                    bail!("tail command failed with: {:?}", status);
                }
                Ok(())
            }
            Err(err) => bail!("Error executing tail command: {}", err),
        }
    }
}
