mod cli;
mod client;
mod config;
mod model;
mod propose;

use anyhow::Result;

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<()> {
    cli::run().await
}
