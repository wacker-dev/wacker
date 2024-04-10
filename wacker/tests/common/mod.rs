use std::fs::{create_dir_all, remove_dir_all};
use std::time::Duration;
use tokio::net::UnixListener;
use tokio::sync::broadcast::{channel, Receiver, Sender};
use tokio::time::sleep;
use tokio_stream::wrappers::UnixListenerStream;
use tonic::transport::Channel;
use wacker::{new_client_with_path, new_service, utils::generate_random_string, WackerClient};

pub struct Server {
    sender: Sender<()>,
    receiver: Receiver<()>,
    dir: String,
}

impl Server {
    pub fn new() -> Self {
        let (sender, receiver) = channel(1);
        Self {
            sender,
            receiver,
            dir: format!("wacker-test-{}", generate_random_string(5)),
        }
    }

    pub async fn start(&mut self) {
        let sock_path = format!("{}/wacker.sock", self.dir);
        let logs_dir = format!("{}/logs", self.dir);
        let db_path = format!("{}/db", self.dir);

        create_dir_all(logs_dir.clone()).expect("create dir failed");
        create_dir_all(db_path.clone()).expect("create dir failed");

        let uds = UnixListener::bind(sock_path.clone()).unwrap();
        let uds_stream = UnixListenerStream::new(uds);

        let dir = self.dir.clone();
        let mut receiver = self.receiver.resubscribe();
        println!("server listening on {:?}", sock_path);
        tokio::spawn(
            tonic::transport::Server::builder()
                .add_service(new_service(sled::open(db_path).unwrap(), logs_dir).await.unwrap())
                .serve_with_incoming_shutdown(uds_stream, async move {
                    receiver.recv().await.expect("");
                    println!("Shutting down the server");
                    remove_dir_all(dir).expect("remove dir failed");
                }),
        );
    }

    pub async fn client(&self) -> WackerClient<Channel> {
        new_client_with_path(format!("{}/wacker.sock", self.dir)).await.unwrap()
    }

    pub async fn shutdown(&self) {
        let _ = self.sender.send(());
        sleep(Duration::from_secs(1)).await;
    }
}
