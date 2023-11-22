use anyhow::{bail, Result};
use clap::Parser;
use tabled::{
    settings::{object::Columns, Modify, Padding, Style, Width},
    Table, Tabled,
};
use tonic::transport::Channel;
use wacker_api::{modules_client::ModulesClient, ModuleStatus};

#[derive(Parser, PartialEq)]
#[structopt(name = "list", aliases = &["ps"])]
pub struct ListCommand {}

#[derive(Tabled)]
struct Module {
    #[tabled(rename = "ID")]
    id: String,
    #[tabled(rename = "PATH")]
    path: String,
    #[tabled(rename = "STATUS")]
    status: &'static str,
}

impl ListCommand {
    pub async fn execute(self, channel: Channel) -> Result<()> {
        let mut client = ModulesClient::new(channel);
        let request = tonic::Request::new(());
        let response = match client.list(request).await {
            Ok(resp) => resp,
            Err(err) => bail!(err.message().to_string()),
        };

        let mut modules = vec![];
        for res in response.into_inner().modules {
            modules.push(Module {
                id: res.id,
                path: res.path,
                status: ModuleStatus::try_from(res.status).unwrap().as_str_name(),
            })
        }

        let mut table = Table::new(modules);
        table
            .with(Padding::new(0, 2, 0, 0))
            .with(Style::blank())
            // the PATH column
            .with(Modify::new(Columns::single(1)).with(Width::wrap(80).keep_words()));

        println!("{table}");

        Ok(())
    }
}
