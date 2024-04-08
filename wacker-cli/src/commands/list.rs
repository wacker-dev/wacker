use ahash::AHashMap;
use anyhow::{bail, Result};
use clap::Parser;
use once_cell::sync::Lazy;
use tabled::{
    settings::{object::Columns, Modify, Padding, Style, Width},
    Table, Tabled,
};
use tonic::transport::Channel;
use wacker::WackerClient;

#[derive(Parser)]
pub struct ListCommand {}

#[derive(Tabled)]
struct Program {
    #[tabled(rename = "ID")]
    id: String,
    #[tabled(rename = "PATH")]
    path: String,
    #[tabled(rename = "STATUS")]
    status: &'static str,
    #[tabled(rename = "ADDRESS")]
    address: String,
}

static STATUS: Lazy<AHashMap<u32, &'static str>> =
    Lazy::new(|| AHashMap::from([(0, "Running"), (1, "Finished"), (2, "Error"), (3, "Stopped")]));

impl ListCommand {
    pub async fn execute(self, mut client: WackerClient<Channel>) -> Result<()> {
        let response = match client.list(()).await {
            Ok(resp) => resp,
            Err(err) => bail!(err.message().to_string()),
        };

        let mut programs = vec![];
        for res in response.into_inner().programs {
            programs.push(Program {
                id: res.id,
                path: res.path,
                status: STATUS.get(&res.status).unwrap_or(&"Unknown"),
                address: res.addr,
            })
        }

        let mut table = Table::new(programs);
        table
            .with(Padding::new(0, 2, 0, 0))
            .with(Style::blank())
            // the PATH column
            .with(Modify::new(Columns::single(1)).with(Width::wrap(60).keep_words()));

        println!("{table}");

        Ok(())
    }
}
