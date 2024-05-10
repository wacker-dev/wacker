mod common;

use crate::common::TestServer;
use anyhow::Result;
use reqwest::ClientBuilder;
use std::time::Duration;
use tokio::time::sleep;
use tokio_stream::StreamExt;
use wacker::{
    DeleteRequest, LogRequest, RestartRequest, RunRequest, ServeRequest, StopRequest, PROGRAM_STATUS_FINISHED,
    PROGRAM_STATUS_RUNNING, PROGRAM_STATUS_STOPPED,
};

#[tokio::test(flavor = "multi_thread")]
async fn run() -> Result<()> {
    let mut server = TestServer::new();
    server.start().await;

    let mut client = server.client().await;
    client
        .run(RunRequest {
            path: "./tests/wasm/hello.wasm".parse()?,
            args: vec![],
        })
        .await?;
    client
        .run(RunRequest {
            path: "./tests/wasm/cli.wasm".parse()?,
            args: vec!["-a=b".to_string(), "-c=d".to_string()],
        })
        .await?;
    sleep(Duration::from_secs(10)).await;

    let response = client.list(()).await?.into_inner();
    assert_eq!(response.programs[0].status, PROGRAM_STATUS_FINISHED);
    assert_eq!(response.programs[1].status, PROGRAM_STATUS_FINISHED);

    server.shutdown().await;
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn serve() -> Result<()> {
    let mut server = TestServer::new();
    server.start().await;

    let mut client = server.client().await;
    client
        .serve(ServeRequest {
            path: "./tests/wasm/http.wasm".parse()?,
            addr: "localhost:8080".to_string(),
        })
        .await?;
    sleep(Duration::from_secs(1)).await;

    let http_client = ClientBuilder::new()
        .timeout(Duration::from_secs(15))
        .no_proxy()
        .build()?;
    let response = http_client
        .get("http://localhost:8080/api_path")
        .query(&[("hello", "world")])
        .send()
        .await?;
    assert!(response.status().is_success());
    assert_eq!(
        response.text().await?,
        "{\"path\":\"/api_path\",\"query\":{\"hello\":\"world\"}}"
    );

    server.shutdown().await;
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn run_http_path() -> Result<()> {
    let mut server = TestServer::new();
    server.start().await;

    let mut client = server.client().await;
    client
        .run(RunRequest {
            path: "https://raw.githubusercontent.com/wacker-dev/wacker/main/wacker/tests/wasm/time.wasm".parse()?,
            args: vec![],
        })
        .await?;
    client
        .serve(ServeRequest {
            path: "https://raw.githubusercontent.com/wacker-dev/wacker/main/wacker/tests/wasm/http.wasm".parse()?,
            addr: "localhost:8080".to_string(),
        })
        .await?;
    sleep(Duration::from_secs(10)).await;

    let response = client.list(()).await?.into_inner();
    assert_eq!(response.programs[0].status, PROGRAM_STATUS_RUNNING);
    assert_eq!(response.programs[1].status, PROGRAM_STATUS_RUNNING);

    server.shutdown().await;
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn list() -> Result<()> {
    let mut server = TestServer::new();
    server.start().await;

    let mut client = server.client().await;
    client
        .run(RunRequest {
            path: "./tests/wasm/hello.wasm".parse()?,
            args: vec![],
        })
        .await?;

    let response = client.list(()).await?.into_inner();
    assert_eq!(response.programs.len(), 1);

    server.shutdown().await;
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn stop() -> Result<()> {
    let mut server = TestServer::new();
    server.start().await;

    let mut client = server.client().await;
    client
        .run(RunRequest {
            path: "./tests/wasm/time.wasm".parse()?,
            args: vec![],
        })
        .await?;
    sleep(Duration::from_secs(1)).await;

    client
        .stop(StopRequest {
            ids: vec!["t".to_string()],
        })
        .await?;
    sleep(Duration::from_secs(1)).await;

    let response = client.list(()).await?.into_inner();
    assert_eq!(response.programs[0].status, PROGRAM_STATUS_STOPPED);

    server.shutdown().await;
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn restart() -> Result<()> {
    let mut server = TestServer::new();
    server.start().await;

    let mut client = server.client().await;
    let run_resp = client
        .run(RunRequest {
            path: "./tests/wasm/hello.wasm".parse()?,
            args: vec![],
        })
        .await?
        .into_inner();
    let serve_resp = client
        .serve(ServeRequest {
            path: "./tests/wasm/http.wasm".parse()?,
            addr: "localhost:8081".to_string(),
        })
        .await?
        .into_inner();
    sleep(Duration::from_secs(1)).await;

    let response = client
        .restart(RestartRequest {
            ids: vec![run_resp.id, serve_resp.id],
        })
        .await;
    assert!(response.is_ok());

    server.shutdown().await;
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn delete() -> Result<()> {
    let mut server = TestServer::new();
    server.start().await;

    let mut client = server.client().await;
    client
        .run(RunRequest {
            path: "./tests/wasm/hello.wasm".parse()?,
            args: vec![],
        })
        .await?;
    sleep(Duration::from_secs(1)).await;

    client
        .delete(DeleteRequest {
            ids: vec!["h".to_string()],
        })
        .await?;

    let response = client.list(()).await?.into_inner();
    assert_eq!(response.programs.len(), 0);

    server.shutdown().await;
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn delete_ambiguous_id() -> Result<()> {
    let mut server = TestServer::new();
    server.start().await;

    let mut client = server.client().await;
    client
        .run(RunRequest {
            path: "./tests/wasm/hello.wasm".parse()?,
            args: vec![],
        })
        .await?;
    client
        .run(RunRequest {
            path: "./tests/wasm/hello.wasm".parse()?,
            args: vec![],
        })
        .await?;
    sleep(Duration::from_secs(1)).await;

    let response = client
        .delete(DeleteRequest {
            ids: vec!["hello".to_string()],
        })
        .await;
    assert!(response.is_err());
    assert_eq!(
        response.err().unwrap().message(),
        "ambiguous program id hello, more than one program starts with this id"
    );

    server.shutdown().await;
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn logs() -> Result<()> {
    let mut server = TestServer::new();
    server.start().await;

    let mut client = server.client().await;
    client
        .run(RunRequest {
            path: "./tests/wasm/hello.wasm".parse()?,
            args: vec![],
        })
        .await?;
    sleep(Duration::from_secs(3)).await;

    let mut response = client
        .logs(LogRequest {
            id: "hello".to_string(),
            follow: false,
            tail: 1,
        })
        .await?
        .into_inner();
    let item = response.next().await.unwrap();
    assert_eq!(item.unwrap().content, "Hello, world!\n");

    server.shutdown().await;
    Ok(())
}
