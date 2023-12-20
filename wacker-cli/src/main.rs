mod commands;

use anyhow::Result;
use clap::Parser;
use wacker::new_client;

#[derive(Parser)]
#[command(name = "wacker")]
#[command(author, version, about, long_about = None)]
struct Wacker {
    #[command(subcommand)]
    subcommand: Subcommand,
}

#[derive(Parser)]
enum Subcommand {
    /// Runs a WebAssembly module
    Run(commands::RunCommand),
    /// Lists running WebAssembly modules
    #[command(visible_alias = "ps")]
    List(commands::ListCommand),
    /// Stops a WebAssembly module
    Stop(commands::StopCommand),
    /// Restarts a WebAssembly module
    Restart(commands::RestartCommand),
    /// Deletes a WebAssembly module
    #[command(visible_alias = "rm")]
    Delete(commands::DeleteCommand),
    /// Fetches logs of a module
    #[command(visible_alias = "log")]
    Logs(commands::LogsCommand),
}

impl Wacker {
    /// Executes the command.
    async fn execute(self) -> Result<()> {
        let client = new_client().await?;

        match self.subcommand {
            Subcommand::Run(c) => c.execute(client).await,
            Subcommand::List(c) => c.execute(client).await,
            Subcommand::Stop(c) => c.execute(client).await,
            Subcommand::Restart(c) => c.execute(client).await,
            Subcommand::Delete(c) => c.execute(client).await,
            Subcommand::Logs(c) => c.execute(client).await,
        }
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    Wacker::parse().execute().await
}
