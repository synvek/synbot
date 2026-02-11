use anyhow::Result;

mod agent;
mod bus;
mod channels;
mod cli;
mod config;
mod cron;
mod heartbeat;
mod tools;
mod web;

#[tokio::main]
async fn main() -> Result<()> {
    cli::run().await
}
