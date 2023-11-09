mod commands;

use anyhow::Result;
use clap::Parser;
use tokio::net::UnixStream;
use tonic::transport::Endpoint;
use tower::service_fn;

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
    /// List running WebAssembly modules
    List(commands::ListCommand),
    /// Stops a WebAssembly module
    Stop(commands::StopCommand),
    /// Restart a WebAssembly module
    Restart(commands::RestartCommand),
    /// Delete a WebAssembly module
    Delete(commands::DeleteCommand),
}

impl Wacker {
    /// Executes the command.
    pub async fn execute(self) -> Result<()> {
        let home_dir = dirs::home_dir().expect("Can't get home dir");
        let path = home_dir.join(".wacker/wacker.sock");

        let channel = Endpoint::try_from("http://[::]:50051")?
            .connect_with_connector(service_fn(move |_| {
                // Connect to a Uds socket
                UnixStream::connect(path.to_str().unwrap().to_string())
            }))
            .await?;

        match self.subcommand {
            Subcommand::Run(c) => c.execute(channel).await,
            Subcommand::List(c) => c.execute(channel).await,
            Subcommand::Stop(c) => c.execute(channel).await,
            Subcommand::Restart(c) => c.execute(channel).await,
            Subcommand::Delete(c) => c.execute(channel).await,
        }
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    return Wacker::parse().execute().await;
}
