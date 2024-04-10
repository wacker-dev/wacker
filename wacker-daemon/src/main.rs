use anyhow::{bail, Result};
use chrono::Local;
use clap::Parser;
use env_logger::{Builder, WriteStyle};
use log::{info, LevelFilter};
use std::fs::{create_dir_all, remove_file};
use std::io::Write;
use tokio::net::UnixListener;
use tokio::signal;
use tokio_stream::wrappers::UnixListenerStream;
use wacker::{get_db_path, get_logs_dir, get_sock_path, new_service};

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
        let sock_path = get_sock_path()?;
        if sock_path.exists() {
            bail!("wackerd socket file exists, is wackerd already running?");
        }

        let logs_dir = get_logs_dir()?;
        if !logs_dir.exists() {
            create_dir_all(logs_dir)?;
        }

        let uds = UnixListener::bind(sock_path)?;
        let uds_stream = UnixListenerStream::new(uds);

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

        let db = sled::open(get_db_path()?)?;

        info!("server listening on {:?}", sock_path);
        Ok(tonic::transport::Server::builder()
            .add_service(new_service(db.clone(), logs_dir).await?)
            .serve_with_incoming_shutdown(uds_stream, async {
                signal::ctrl_c().await.expect("failed to listen for event");
                println!();
                info!("Shutting down the server");
                remove_file(sock_path).expect("failed to remove existing socket file");
                db.flush_async().await.expect("failed to flush the db");
            })
            .await?)
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    WackerDaemon::parse().execute().await
}
