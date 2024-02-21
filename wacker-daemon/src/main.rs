use anyhow::{bail, Result};
use chrono::Local;
use clap::Parser;
use env_logger::{Builder, WriteStyle};
use log::{info, LevelFilter};
use std::fs::{create_dir, create_dir_all, remove_file};
use std::io::Write;
use tokio::net::UnixListener;
use tokio::signal;
use tokio_stream::wrappers::UnixListenerStream;
use tonic::codec::CompressionEncoding;
use wacker::{Config, ModulesServer, Server};

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

        Builder::new()
            .format(|buf, record| {
                writeln!(
                    buf,
                    "[{} {} {}] {}",
                    Local::now().format("%Y-%m-%d %H:%M:%S"),
                    record.level(),
                    record.target(),
                    record.args(),
                )
            })
            .filter_level(LevelFilter::Info)
            .write_style(WriteStyle::Never)
            .init();

        let service = ModulesServer::new(server.clone())
            .send_compressed(CompressionEncoding::Zstd)
            .accept_compressed(CompressionEncoding::Zstd);

        info!("server listening on {:?}", config.sock_path);
        tonic::transport::Server::builder()
            .add_service(service)
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
