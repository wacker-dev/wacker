use anyhow::{bail, Result};
use clap::Parser;
use log::info;
use std::fs::{create_dir, create_dir_all, remove_file};
use tokio::net::UnixListener;
use tokio::signal;
use tokio_stream::wrappers::UnixListenerStream;
use wacker::{Config, ModulesServer, Server};

#[derive(Parser)]
#[command(name = "wackerd")]
#[command(author, version, about, long_about = None)]
struct WackerDaemon {}

impl WackerDaemon {
    async fn execute(self) -> Result<()> {
        let config = Config::new()?;
        if config.sock_path.exists() {
            bail!("wackerd socket file exists, is wackerd already running?");
        }

        let parent_path = config.sock_path.parent().unwrap();
        if !parent_path.exists() {
            create_dir_all(parent_path)?;
        }
        if !config.logs_dir.exists() {
            create_dir(config.logs_dir.clone())?;
        }

        let uds = UnixListener::bind(config.sock_path.clone())?;
        let uds_stream = UnixListenerStream::new(uds);

        let server = Server::new(config.clone()).await?;

        let env = env_logger::Env::default()
            .filter_or("LOG_LEVEL", "info")
            .write_style_or("LOG_STYLE", "never");
        env_logger::init_from_env(env);

        info!("server listening on {:?}", config.sock_path);
        tonic::transport::Server::builder()
            .add_service(ModulesServer::new(server.clone()))
            .serve_with_incoming_shutdown(uds_stream, async {
                signal::ctrl_c().await.expect("failed to listen for event");
                println!();
                info!("Shutting down the server");
                remove_file(config.sock_path).expect("failed to remove existing socket file");
                server.flush_db().await.expect("failed to flush the db");
            })
            .await?;

        Ok(())
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    WackerDaemon::parse().execute().await
}
