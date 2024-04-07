mod commands;

use anyhow::Result;
use clap::Parser;
use wacker::new_client;

#[derive(Parser)]
#[command(name = "wacker")]
#[command(author, version = version(), about, long_about = None)]
struct Wacker {
    #[command(subcommand)]
    subcommand: Subcommand,
}

fn version() -> &'static str {
    // If WACKER_VERSION_INFO is set, use it, otherwise use CARGO_PKG_VERSION.
    option_env!("WACKER_VERSION_INFO").unwrap_or(env!("CARGO_PKG_VERSION"))
}

#[derive(Parser)]
enum Subcommand {
    /// Runs a WebAssembly program
    Run(commands::RunCommand),
    /// Serves an HTTP WebAssembly program
    Serve(commands::ServeCommand),
    /// Lists running WebAssembly programs
    #[command(visible_alias = "ps")]
    List(commands::ListCommand),
    /// Stops a WebAssembly program
    Stop(commands::StopCommand),
    /// Restarts a WebAssembly program
    Restart(commands::RestartCommand),
    /// Deletes a WebAssembly program
    #[command(visible_alias = "rm")]
    Delete(commands::DeleteCommand),
    /// Fetches logs of a program
    #[command(visible_alias = "log")]
    Logs(commands::LogsCommand),
}

impl Wacker {
    /// Executes the command.
    async fn execute(self) -> Result<()> {
        let client = new_client().await?;

        match self.subcommand {
            Subcommand::Run(c) => c.execute(client).await,
            Subcommand::Serve(c) => c.execute(client).await,
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
