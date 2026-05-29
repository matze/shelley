mod cli;
mod client;
mod config;
mod model;

use anyhow::Result;

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<()> {
    cli::run().await
}
