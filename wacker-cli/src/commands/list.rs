use anyhow::{bail, Result};
use clap::Parser;
use tabled::{
    settings::{object::Columns, Modify, Padding, Style, Width},
    Table, Tabled,
};
use tonic::transport::Channel;
use wacker::{ModuleStatus, ModulesClient};

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
                status: ModuleStatus::try_from(res.status).unwrap().as_str_name(),
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
