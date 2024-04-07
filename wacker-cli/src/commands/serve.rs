use anyhow::{anyhow, Result};
use clap::Parser;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use tonic::transport::Channel;
use wacker::{ServeRequest, WackerClient};

const DEFAULT_ADDR: SocketAddr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0)), 8080);

#[derive(Parser)]
pub struct ServeCommand {
    /// Program file path
    #[arg(required = true)]
    path: String,
    /// Socket address for the web server to bind to
    #[arg(long = "addr", default_value_t = DEFAULT_ADDR )]
    addr: SocketAddr,
}

impl ServeCommand {
    /// Executes the command.
    pub async fn execute(self, mut client: WackerClient<Channel>) -> Result<()> {
        match client
            .serve(ServeRequest {
                path: self.path.to_string(),
                addr: self.addr.to_string(),
            })
            .await
        {
            Ok(_) => Ok(()),
            Err(err) => Err(anyhow!(err.message().to_string())),
        }
    }
}
