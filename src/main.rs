use anyhow::Result;

mod agent;
mod bus;
mod channels;
mod cli;
mod config;
mod cron;
mod heartbeat;
mod logging;
mod tools;
mod web;
mod sandbox;

mod url_utils;

#[cfg(target_os = "windows")]
mod appcontainer_dns;

#[tokio::main]
async fn main() -> Result<()> {
    cli::run().await
}
