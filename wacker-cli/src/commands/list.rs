use anyhow::Result;
use clap::Parser;
use tabled::{
    settings::{Padding, Style},
    Table, Tabled,
};
use tonic::transport::Channel;
use wacker_api::{modules_client::ModulesClient, ModuleStatus};

#[derive(Parser, PartialEq)]
#[structopt(name = "list", aliases = &["ps"])]
pub struct ListCommand {}

#[derive(Tabled)]
struct Module {
    #[tabled(rename = "NAME")]
    name: String,
    #[tabled(rename = "STATUS")]
    status: &'static str,
}

impl ListCommand {
    pub async fn execute(self, channel: Channel) -> Result<()> {
        let mut client = ModulesClient::new(channel);
        let request = tonic::Request::new(());
        let response = client.list(request).await?;

        let mut modules = vec![];
        for res in response.into_inner().modules {
            modules.push(Module {
                name: res.name,
                status: ModuleStatus::try_from(res.status).unwrap().as_str_name(),
            })
        }

        let mut table = Table::new(modules);
        table.with(Padding::new(0, 2, 0, 0)).with(Style::blank());
        println!("{table}");

        Ok(())
    }
}
