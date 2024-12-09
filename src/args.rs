use std::net::IpAddr;

use clap::{Parser, Subcommand};

#[derive(Debug, Parser)]
pub struct Args {
    #[clap(subcommand)]
    pub command: Command,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    Server {
        #[clap(subcommand)]
        command: ServerCommand,
    },
    Cli,
}

#[derive(Debug, Subcommand)]
pub enum ServerCommand {
    Start {
        #[clap(default_value = "::")]
        addr: IpAddr,
        #[clap(default_value = "9988")]
        port: u16,
    },
    Stop {
        #[clap(default_value = "127.0.0.1")]
        addr: IpAddr,
        #[clap(default_value = "9988")]
        port: u16,
    },
}
