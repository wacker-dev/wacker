mod config;
mod runtime;
mod server;
pub mod utils;
mod proto {
    tonic::include_proto!("wacker");
}

use anyhow::Result;
use sled::Db;
use std::path::Path;
use tokio::net::UnixStream;
use tonic::{
    codec::CompressionEncoding,
    transport::{Channel, Endpoint},
};
use tower::service_fn;

pub use self::config::*;
pub use self::proto::{
    wacker_client::WackerClient, wacker_server::Wacker, wacker_server::WackerServer, DeleteRequest, ListResponse,
    LogRequest, LogResponse, Program, ProgramResponse, RestartRequest, RunRequest, ServeRequest, StopRequest,
};
pub use self::server::*;

pub const PROGRAM_STATUS_RUNNING: u32 = 0;
pub const PROGRAM_STATUS_FINISHED: u32 = 1;
pub const PROGRAM_STATUS_ERROR: u32 = 2;
pub const PROGRAM_STATUS_STOPPED: u32 = 3;

pub async fn new_service<P: AsRef<Path>>(db: Db, logs_dir: P) -> Result<WackerServer<Server>> {
    Ok(WackerServer::new(Server::new(db, logs_dir).await?)
        .send_compressed(CompressionEncoding::Zstd)
        .accept_compressed(CompressionEncoding::Zstd))
}

pub async fn new_client() -> Result<WackerClient<Channel>> {
    new_client_with_path(get_sock_path()?).await
}

pub async fn new_client_with_path<P: AsRef<Path>>(sock_path: P) -> Result<WackerClient<Channel>> {
    let path = sock_path.as_ref().to_path_buf();
    // We will ignore this uri because uds do not use it
    let channel = Endpoint::try_from("unix://wacker")?
        .connect_with_connector(service_fn(move |_| {
            // Connect to a Uds socket
            UnixStream::connect(path.clone())
        }))
        .await?;

    Ok(WackerClient::new(channel)
        .send_compressed(CompressionEncoding::Zstd)
        .accept_compressed(CompressionEncoding::Zstd))
}
