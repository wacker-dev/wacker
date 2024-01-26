mod config;
mod runtime;
mod server;
mod utils;
mod module {
    tonic::include_proto!("module");
}

use anyhow::Result;
use tokio::net::UnixStream;
use tonic::transport::{Channel, Endpoint};
use tower::service_fn;

pub use self::config::*;
pub use self::module::{
    modules_client::ModulesClient, modules_server::ModulesServer, DeleteRequest, ListResponse, LogRequest, LogResponse,
    Module, ModuleStatus, ModuleType, RestartRequest, RunRequest, ServeRequest, StopRequest,
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

    Ok(ModulesClient::new(channel))
}
