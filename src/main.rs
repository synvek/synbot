//For matrix sdk dependency issue
#![recursion_limit = "512"]
#![type_length_limit = "16777216"]

use anyhow::Result;

#[cfg(feature = "acp")]
mod acp;
mod agent;
mod appcontainer_dns;
mod background;
mod bus;
mod channels;
mod checkpoint;
mod cli;
mod codeindex;
mod commands;
mod config;
mod cron;
mod git;
mod heartbeat;
mod hooks;
mod logging;
mod office;
mod plugin;
mod project;
mod rig_provider;
mod sandbox;
mod security;
mod tasks;
mod terminal;
mod tools;
mod url_utils;
mod web;
mod workflow;

#[tokio::main]
async fn main() -> Result<()> {
    cli::run().await
}
