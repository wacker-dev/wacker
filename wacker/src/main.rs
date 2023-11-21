mod module;
mod run;

use crate::module::Service;
use anyhow::Result;
use log::info;
use std::fs;
use tokio::net::UnixListener;
use tokio_stream::wrappers::UnixListenerStream;
use tonic::transport::Server;
use wacker_api::modules_server::ModulesServer;

#[tokio::main]
async fn main() -> Result<()> {
    let home_dir = dirs::home_dir().expect("Can't get home dir");
    let binding = home_dir.join(".wacker/wacker.sock");
    let path = binding.as_path();
    let parent_path = path.parent().unwrap();

    if !parent_path.exists() {
        fs::create_dir_all(parent_path)?;
    }
    if path.exists() {
        fs::remove_file(path).expect("Failed to remove existing socket file");
    }

    let uds = UnixListener::bind(path)?;
    let uds_stream = UnixListenerStream::new(uds);

    let inner = Service::new(home_dir)?;

    let env = env_logger::Env::default()
        .filter_or("LOG_LEVEL", "info")
        .write_style_or("LOG_STYLE", "never");
    env_logger::init_from_env(env);

    info!("server listening on {:?}", path);
    Server::builder()
        .add_service(ModulesServer::new(inner))
        .serve_with_incoming(uds_stream)
        .await?;

    Ok(())
}
