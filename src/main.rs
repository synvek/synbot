use anyhow::Result;

mod agent;
mod background;
mod bus;
mod channels;
mod cli;
mod config;
mod cron;
mod heartbeat;
mod hooks;
mod logging;
mod plugin;
mod rig_provider;
mod tools;
mod web;
mod sandbox;

mod url_utils;

mod appcontainer_dns;

#[tokio::main]
async fn main() -> Result<()> {
    cli::run().await
}
