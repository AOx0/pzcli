// #![deny(clippy::unwrap_used)]

use args::Args;
use clap::Parser;

mod args;
mod cli;
mod pz;
mod server;

#[tokio::main]
async fn main() {
    let args = Args::parse();
    env_logger::init();

    log::debug!("Args: {:?}", args);
    match args.command {
        args::Command::Server { command } => server::perform(command).await,
        args::Command::Cli { addr, port } => cli::prompt(addr, port).await,
    }
}
