mod runtime;
mod server;
pub mod utils;
mod proto {
    tonic::include_proto!("wacker");
}

use anyhow::{anyhow, bail, Result};
use chrono::Local;
use env_logger::{Builder, Target, WriteStyle};
use log::{info, warn, LevelFilter};
use std::fs::{create_dir_all, remove_file};
use std::future::Future;
use std::io::{ErrorKind, Write};
use std::path::{Path, PathBuf};
use tokio::net::{UnixListener, UnixStream};
use tokio_stream::wrappers::UnixListenerStream;
use tonic::{
    codec::CompressionEncoding,
    transport::{Channel, Endpoint},
};
use tower::service_fn;

pub use self::proto::{
    wacker_client::WackerClient as Client, DeleteRequest, ListResponse, LogRequest, LogResponse, Program,
    ProgramResponse, RestartRequest, RunRequest, ServeRequest, StopRequest,
};

pub const PROGRAM_STATUS_RUNNING: u32 = 0;
pub const PROGRAM_STATUS_FINISHED: u32 = 1;
pub const PROGRAM_STATUS_ERROR: u32 = 2;
pub const PROGRAM_STATUS_STOPPED: u32 = 3;

pub const PROGRAM_TYPE_WASI: u32 = 0;
pub const PROGRAM_TYPE_HTTP: u32 = 1;

fn get_main_dir() -> Result<PathBuf> {
    match dirs::home_dir() {
        Some(home_dir) => Ok(home_dir.join(".wacker")),
        None => Err(anyhow!("can't get home dir")),
    }
}

#[derive(Default)]
pub struct Server {
    main_dir: Option<PathBuf>,
    is_test: bool,
}

impl Server {
    pub fn new() -> Self {
        Default::default()
    }

    pub fn with_dir<P: AsRef<Path>>(&mut self, dir: P) -> &mut Self {
        self.main_dir = Some(dir.as_ref().to_path_buf());
        self
    }

    pub fn is_test(&mut self, is_test: bool) -> &mut Self {
        self.is_test = is_test;
        self
    }

    pub async fn start<F: Future<Output = ()> + Send + 'static>(&self, shutdown: F) -> Result<()> {
        let main_dir = match &self.main_dir {
            Some(p) => p.clone(),
            None => get_main_dir()?,
        };

        let sock_path = main_dir.join("wacker.sock");
        if sock_path.exists() {
            bail!("wackerd socket file exists, is wackerd already running?");
        }

        let logs_dir = main_dir.join("logs");
        if !logs_dir.exists() {
            create_dir_all(logs_dir.as_path())?;
        }

        let db_path = main_dir.join("db");
        let db = sled::open(db_path)?;

        let mut log_builder = Builder::new();
        log_builder
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
            .target(Target::Stdout);
        if self.is_test {
            let _ = log_builder.is_test(true).try_init();
        } else {
            log_builder.try_init()?;
        }

        let uds = UnixListener::bind(sock_path.as_path())?;
        let uds_stream = UnixListenerStream::new(uds);
        let service = proto::wacker_server::WackerServer::new(server::Server::new(db.clone(), logs_dir).await?)
            .send_compressed(CompressionEncoding::Zstd)
            .accept_compressed(CompressionEncoding::Zstd)
            .send_compressed(CompressionEncoding::Gzip)
            .accept_compressed(CompressionEncoding::Gzip);

        info!("server listening on {:?}", sock_path.as_path());

        let run = tonic::transport::Server::builder()
            .add_service(service)
            .serve_with_incoming_shutdown(uds_stream, async move {
                shutdown.await;
                info!("Shutting down the server");
                if let Err(err) = remove_file(sock_path) {
                    if err.kind() != ErrorKind::NotFound {
                        warn!("failed to remove existing socket file: {}", err);
                    }
                }
                if let Err(err) = db.flush_async().await {
                    warn!("failed to flush the db: {}", err);
                }
            });

        if self.is_test {
            tokio::spawn(run);
        } else {
            run.await?;
        }
        Ok(())
    }
}

pub async fn new_client() -> Result<Client<Channel>> {
    new_client_with_path(get_main_dir()?.join("wacker.sock")).await
}

pub async fn new_client_with_path<P: AsRef<Path>>(sock_path: P) -> Result<Client<Channel>> {
    let path = sock_path.as_ref().to_path_buf();
    // We will ignore this uri because uds do not use it
    let channel = Endpoint::try_from("unix://wacker")?
        .connect_with_connector(service_fn(move |_| {
            // Connect to a Uds socket
            UnixStream::connect(path.clone())
        }))
        .await?;

    Ok(Client::new(channel)
        .send_compressed(CompressionEncoding::Zstd)
        .accept_compressed(CompressionEncoding::Zstd)
        .accept_compressed(CompressionEncoding::Gzip))
}
