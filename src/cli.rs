use std::io;

use anyhow::Result;
use clap::{CommandFactory, Parser, Subcommand};
use clap_complete::Shell;

#[derive(Parser)]
#[command(
    name = "shelley",
    version,
    about = "A minimal oneshot shell agent: propose commands or answer read-only questions"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    #[command(about = "Propose shell commands for a task; never executes them")]
    Propose {
        #[arg(required = true, num_args = 1.., help = "Natural-language task description")]
        query: Vec<String>,
    },
    #[command(about = "Answer a question using read-only tools (files, web)")]
    Ask {
        #[arg(required = true, num_args = 1.., help = "Natural-language question")]
        query: Vec<String>,
    },
    #[command(about = "Generate a shell completion script")]
    Completions {
        #[arg(value_enum, help = "Target shell")]
        shell: Shell,
    },
}

pub async fn run() -> Result<()> {
    match Cli::parse().command {
        Command::Propose { query } => propose(query.join(" ")).await,
        Command::Ask { query } => ask(query.join(" ")).await,
        Command::Completions { shell } => Ok(generate_completions(shell)),
    }
}

fn generate_completions(shell: Shell) {
    let mut cmd = Cli::command();
    let name = cmd.get_name().to_string();
    clap_complete::generate(shell, &mut cmd, name, &mut io::stdout());
}

async fn propose(_query: String) -> Result<()> {
    todo!()
}

async fn ask(_query: String) -> Result<()> {
    todo!()
}
