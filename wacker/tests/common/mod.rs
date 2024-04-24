use std::fs::remove_dir_all;
use std::time::Duration;
use tokio::sync::broadcast::{channel, Receiver, Sender};
use tokio::time::sleep;
use tonic::transport::Channel;
use wacker::{new_client_with_path, utils::generate_random_string, Client, Server};

pub struct TestServer {
    sender: Sender<()>,
    receiver: Receiver<()>,
    dir: String,
}

impl TestServer {
    pub fn new() -> Self {
        let (sender, receiver) = channel(1);
        Self {
            sender,
            receiver,
            dir: format!("wacker-test-{}", generate_random_string(5)),
        }
    }

    pub async fn start(&mut self) {
        let dir = self.dir.clone();
        let mut receiver = self.receiver.resubscribe();

        Server::new()
            .with_dir(dir.clone())
            .is_test(true)
            .start(async move {
                receiver.recv().await.expect("");
                remove_dir_all(dir).expect("remove dir failed");
            })
            .await
            .unwrap();
    }

    pub async fn client(&self) -> Client<Channel> {
        new_client_with_path(format!("{}/wacker.sock", self.dir)).await.unwrap()
    }

    pub async fn shutdown(&self) {
        let _ = self.sender.send(());
        sleep(Duration::from_secs(1)).await;
    }
}
