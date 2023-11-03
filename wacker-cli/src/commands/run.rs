use anyhow::Result;
use clap::Parser;
use std::path::Path;
use tokio::net::UnixStream;
use tonic::transport::Endpoint;
use tower::service_fn;
use wacker_api::{module_client::ModuleClient, RunRequest};

#[derive(Parser, PartialEq)]
#[structopt(name = "run")]
pub struct RunCommand {
    /// Module file path
    #[arg(required = true)]
    path: String,
}

impl RunCommand {
    /// Executes the command.
    pub async fn execute(self) -> Result<()> {
        let home_dir = dirs::home_dir().expect("Can't get home dir");
        let path = home_dir.join(".wacker/wacker.sock");

        let channel = Endpoint::try_from("http://[::]:50051")?
            .connect_with_connector(service_fn(move |_| {
                // Connect to a Uds socket
                UnixStream::connect(path.to_str().unwrap().to_string())
            }))
            .await?;
        let mut client = ModuleClient::new(channel);

        let path = Path::new(self.path.as_str());
        let request = tonic::Request::new(RunRequest {
            name: path.file_name().unwrap().to_str().unwrap().to_string(),
            path: path.to_str().unwrap().to_string(),
        });

        let response = client.run(request).await?;

        println!("RESPONSE={:?}", response);

        Ok(())
    }
}
