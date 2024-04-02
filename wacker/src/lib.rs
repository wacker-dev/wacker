mod config;
mod runtime;
mod server;
mod utils;
mod module {
    tonic::include_proto!("module");

    pub const MODULE_TYPE_WASI: u32 = 0;
    pub const MODULE_TYPE_HTTP: u32 = 1;

    pub const MODULE_STATUS_RUNNING: u32 = 0;
    pub const MODULE_STATUS_FINISHED: u32 = 1;
    pub const MODULE_STATUS_ERROR: u32 = 2;
    pub const MODULE_STATUS_STOPPED: u32 = 3;
}

use anyhow::Result;
use tokio::net::UnixStream;
use tonic::codec::CompressionEncoding;
use tonic::transport::{Channel, Endpoint};
use tower::service_fn;

pub use self::config::*;
pub use self::module::{
    modules_client::ModulesClient, modules_server::ModulesServer, DeleteRequest, ListResponse, LogRequest, LogResponse,
    Module, RestartRequest, RunRequest, ServeRequest, StopRequest, MODULE_STATUS_ERROR, MODULE_STATUS_FINISHED,
    MODULE_STATUS_RUNNING, MODULE_STATUS_STOPPED, MODULE_TYPE_HTTP, MODULE_TYPE_WASI,
};
pub use self::server::*;

pub async fn new_client() -> Result<ModulesClient<Channel>> {
    let config = Config::new()?;

    let channel = Endpoint::try_from("http://[::]:50051")?
        .connect_with_connector(service_fn(move |_| {
            // Connect to a Uds socket
            UnixStream::connect(config.sock_path.to_str().unwrap().to_string())
        }))
        .await?;

    Ok(ModulesClient::new(channel)
        .send_compressed(CompressionEncoding::Zstd)
        .accept_compressed(CompressionEncoding::Zstd))
}
