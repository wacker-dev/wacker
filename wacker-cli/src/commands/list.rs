use anyhow::{bail, Result};
use clap::Parser;
use once_cell::sync::Lazy;
use std::collections::HashMap;
use tabled::{
    settings::{object::Columns, Modify, Padding, Style, Width},
    Table, Tabled,
};
use tonic::transport::Channel;
use wacker::ModulesClient;

#[derive(Parser)]
pub struct ListCommand {}

#[derive(Tabled)]
struct Module {
    #[tabled(rename = "ID")]
    id: String,
    #[tabled(rename = "PATH")]
    path: String,
    #[tabled(rename = "STATUS")]
    status: &'static str,
    #[tabled(rename = "ADDRESS")]
    address: String,
}

static STATUS: Lazy<HashMap<u32, &'static str>> = Lazy::new(|| {
    let mut table = HashMap::new();
    table.insert(0, "Running");
    table.insert(1, "Finished");
    table.insert(2, "Error");
    table.insert(3, "Stopped");
    table
});

impl ListCommand {
    pub async fn execute(self, mut client: ModulesClient<Channel>) -> Result<()> {
        let response = match client.list(()).await {
            Ok(resp) => resp,
            Err(err) => bail!(err.message().to_string()),
        };

        let mut modules = vec![];
        for res in response.into_inner().modules {
            modules.push(Module {
                id: res.id,
                path: res.path,
                status: STATUS.get(&res.status).unwrap_or(&"Unknown"),
                address: res.addr,
            })
        }

        let mut table = Table::new(modules);
        table
            .with(Padding::new(0, 2, 0, 0))
            .with(Style::blank())
            // the PATH column
            .with(Modify::new(Columns::single(1)).with(Width::wrap(60).keep_words()));

        println!("{table}");

        Ok(())
    }
}
