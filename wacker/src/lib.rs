mod config;
mod runtime;
mod server;
mod utils;
mod proto {
    tonic::include_proto!("wacker");
}

use anyhow::Result;
use tokio::net::UnixStream;
use tonic::codec::CompressionEncoding;
use tonic::transport::{Channel, Endpoint};
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

pub async fn new_client() -> Result<WackerClient<Channel>> {
    let sock_path = get_sock_path()?;

    let channel = Endpoint::try_from("http://[::]:50051")?
        .connect_with_connector(service_fn(move |_| {
            // Connect to a Uds socket
            UnixStream::connect(sock_path)
        }))
        .await?;

    Ok(WackerClient::new(channel)
        .send_compressed(CompressionEncoding::Zstd)
        .accept_compressed(CompressionEncoding::Zstd))
}
