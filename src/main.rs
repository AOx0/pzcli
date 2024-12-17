// #![deny(clippy::unwrap_used)]

use std::env;

use args::Args;
use clap::Parser;

mod args;
mod cli;
mod pz;
mod server;

#[tokio::main]
async fn main() {
    let args = Args::parse();

    if env::var("RUST_LOG").is_err() {
        env::set_var("RUST_LOG", "info")
    }
    env_logger::init();

    log::debug!("Args: {:?}", args);
    match args.command {
        args::Command::Server { command } => server::perform(command).await,
        args::Command::Cli { addr, port } => cli::prompt(addr, port).await,
    }
}
