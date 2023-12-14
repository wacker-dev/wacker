pub mod config;

mod module {
    tonic::include_proto!("module");
}
pub use self::module::*;

use anyhow::{bail, Result};
use tokio::net::UnixStream;
use tonic::transport::{Channel, Endpoint};
use tower::service_fn;

pub async fn new_client() -> Result<modules_client::ModulesClient<Channel>> {
    let home_dir = dirs::home_dir();
    if home_dir.is_none() {
        bail!("can't get home dir");
    }
    let path = home_dir.unwrap().join(config::SOCK_PATH);

    let channel = Endpoint::try_from("http://[::]:50051")?
        .connect_with_connector(service_fn(move |_| {
            // Connect to a Uds socket
            UnixStream::connect(path.to_str().unwrap().to_string())
        }))
        .await?;

    Ok(modules_client::ModulesClient::new(channel))
}
