//For matrix sdk dependency issue
#![recursion_limit = "512"]
#![type_length_limit = "16777216"]

use anyhow::Result;

mod agent;
mod appcontainer_dns;
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
mod sandbox;
mod security;
mod tools;
mod url_utils;
mod web;
mod workflow;

#[tokio::main]
async fn main() -> Result<()> {
    cli::run().await
}
