mod commands;

use anyhow::Result;
use clap::Parser;

#[derive(Parser)]
#[command(name = "wacker")]
#[command(author = "ia")]
#[command(version = "0.1.0")]
#[command(about = "wacker client", long_about = None)]
struct Wacker {
    #[clap(subcommand)]
    subcommand: Subcommand,
}

#[derive(Parser, PartialEq)]
enum Subcommand {
    /// Runs a WebAssembly module
    Run(commands::RunCommand),
}

impl Wacker {
    /// Executes the command.
    pub async fn execute(self) -> Result<()> {
        match self.subcommand {
            Subcommand::Run(c) => c.execute().await,
        }
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    return Wacker::parse().execute().await;
}
