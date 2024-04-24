use anyhow::Result;
use clap::Parser;
use tokio::signal;
use wacker::Server;

#[derive(Parser)]
#[command(name = "wackerd")]
#[command(author, version = version(), about, long_about = None)]
struct WackerDaemon {}

fn version() -> &'static str {
    // If WACKER_VERSION_INFO is set, use it, otherwise use CARGO_PKG_VERSION.
    option_env!("WACKER_VERSION_INFO").unwrap_or(env!("CARGO_PKG_VERSION"))
}

impl WackerDaemon {
    async fn execute(self) -> Result<()> {
        Server::new()
            .start(async {
                signal::ctrl_c().await.expect("failed to listen for event");
            })
            .await
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    WackerDaemon::parse().execute().await
}
