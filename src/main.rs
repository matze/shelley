mod ask;
mod cli;
mod client;
mod config;
mod model;
mod propose;
mod render;
mod shell;
mod tools;
mod ui;

use anyhow::Result;

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<()> {
    cli::run().await
}
