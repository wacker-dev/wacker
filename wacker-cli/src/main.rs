mod commands;

use anyhow::Result;
use clap::Parser;
use tokio::net::UnixStream;
use tonic::transport::Endpoint;
use tower::service_fn;
use wacker_api::config::SOCK_PATH;

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
        let home_dir = dirs::home_dir().expect("Can't get home dir");
        let path = home_dir.join(SOCK_PATH);

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
            Subcommand::Logs(c) => c.execute(channel).await,
        }
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    Wacker::parse().execute().await
}
