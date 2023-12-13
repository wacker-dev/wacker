mod runtime;
mod server;

use crate::server::Server;
use anyhow::{bail, Result};
use clap::Parser;
use log::info;
use std::fs;
use tokio::net::UnixListener;
use tokio::signal;
use tokio_stream::wrappers::UnixListenerStream;
use wacker_api::{
    config::{DB_PATH, SOCK_PATH},
    modules_server::ModulesServer,
};

#[derive(Parser)]
#[command(name = "wackerd")]
#[command(author, version, about, long_about = None)]
struct WackerDaemon {}

impl WackerDaemon {
    async fn execute(self) -> Result<()> {
        let home_dir = dirs::home_dir().expect("Can't get home dir");
        let binding = home_dir.join(SOCK_PATH);
        let path = binding.as_path();
        let parent_path = path.parent().unwrap();

        if !parent_path.exists() {
            fs::create_dir_all(parent_path)?;
        }
        if path.exists() {
            bail!("wackerd socket file exists, is wackerd already running?");
        }

        let uds = UnixListener::bind(path)?;
        let uds_stream = UnixListenerStream::new(uds);

        let db = sled::open(home_dir.join(DB_PATH))?;
        let inner = Server::new(home_dir, db.clone()).await?;

        let env = env_logger::Env::default()
            .filter_or("LOG_LEVEL", "info")
            .write_style_or("LOG_STYLE", "never");
        env_logger::init_from_env(env);

        info!("server listening on {:?}", path);
        tonic::transport::Server::builder()
            .add_service(ModulesServer::new(inner))
            .serve_with_incoming_shutdown(uds_stream, async {
                signal::ctrl_c().await.expect("failed to listen for event");
                println!();
                info!("Shutting down the server");
                fs::remove_file(path).expect("failed to remove existing socket file");
                db.flush_async().await.expect("failed to flush the db");
            })
            .await?;

        Ok(())
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    WackerDaemon::parse().execute().await
}
