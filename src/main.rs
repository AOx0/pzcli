// #![deny(clippy::unwrap_used)]

use std::{net::SocketAddr, process::Stdio, sync::Arc, time::Duration};

use args::{Args, ServerCommand};
use clap::Parser;
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader, BufWriter},
    net::{TcpListener, TcpStream},
    process::Command,
    select,
    sync::{mpsc, oneshot, watch},
    task::JoinSet,
};

mod args;

#[tokio::main]
async fn main() {
    let args = Args::parse();
    env_logger::init();

    log::debug!("Args: {:?}", args);
    match args.command {
        args::Command::Server { command } => perform_server_command(command).await,
        args::Command::Cli => todo!(),
    }
}

struct PZChild {
    tx: mpsc::Sender<(String, oneshot::Sender<String>)>,
    handlers: JoinSet<()>,
}

impl PZChild {
    pub async fn new(notifier: Arc<watch::Sender<bool>>) -> Result<PZChild, std::io::Error> {
        let (tx, mut rx) = mpsc::channel(100);
        let mut set = JoinSet::new();

        log::debug!("Starting ProjectZomboid64 process via start-server.sh");
        let mut pz = Command::new("bash")
            .current_dir("/home/ae/.steam/steam/steamapps/common/Project Zomboid Dedicated Server")
            .arg("start-server.sh")
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .stdin(Stdio::piped())
            .spawn()?;

        let stdout = pz.stdout.take();
        let stderr = pz.stderr.take();
        let stdin = pz.stdin.take().unwrap();

        tokio::spawn(async move {
            let status = pz.wait().await.expect("child process encountered an error");
            log::error!("child status was: {}", status);
        });

        fn handle_log(line: &str) -> &str {
            let Some((level, msg)) = line.split_once(":") else {
                println!("{line}");
                return line;
            };
            let msg = msg.trim();

            let is_log = ["LOG", "ERROR", "WARN"]
                .iter()
                .any(|str| level.starts_with(str));

            if is_log {
                let level = level.trim();
                let Some((from, msg)) = msg.split_once(",") else {
                    log::info!("{msg}");
                    return msg;
                };
                let from = from.trim();

                let Some((_, msg)) = msg.split_once(">") else {
                    log::info!("{msg}");
                    return msg;
                };

                let Some((n, msg)) = msg.split_once(">") else {
                    log::info!("{msg}");
                    return msg;
                };

                let n = n.trim().replace(",", "");
                let n = n.parse::<u64>().unwrap_or_else(|err| {
                    log::error!("Invalid log number {n}. Error: {err}");
                    0
                });

                match level.trim() {
                    "LOG" => log::info!("{n:x} {from}: {msg}"),
                    "WARN" => log::warn!("{n:x} {from}: {msg}"),
                    "ERROR" => log::error!("{n:x} {from}: {msg}"),
                    _ => log::debug!("{msg}"),
                }

                msg
            } else {
                println!("{msg}");
                msg
            }
        }

        set.spawn(handle_out_lines("stderr", stderr, handle_log));
        set.spawn(handle_out_lines("stdout", stdout, move |line| {
            let msg = handle_log(line);
            if msg.contains("SERVER STARTED") {
                notifier.send(true).unwrap();
                log::info!("Server started!");
            }

            msg
        }));

        set.spawn(async move {
            log::debug!("Starting Tokio task");
            let mut stdin = BufWriter::new(stdin);

            loop {
                let Some((msg, tx)): Option<(String, _)> = rx.recv().await else {
                    continue;
                };

                println!("Sending {msg:?}");
                stdin.write_all(msg.as_bytes()).await.unwrap();
                stdin.flush().await.unwrap();
            }
        });

        Ok(PZChild { tx, handlers: set })
    }

    pub async fn send(
        &self,
        cmd: String,
        tx: oneshot::Sender<String>,
    ) -> Result<(), mpsc::error::SendError<(String, oneshot::Sender<String>)>> {
        self.tx.send((cmd, tx)).await
    }
}

impl Drop for PZChild {
    fn drop(&mut self) {
        let mut handlers = JoinSet::new();

        std::mem::swap(&mut handlers, &mut self.handlers);

        tokio::spawn(async move {
            log::debug!("Waiting for all PZChild tasks");
            handlers.join_all().await;
        });
    }
}

async fn handle_out_lines(
    name: &str,
    out: Option<impl tokio::io::AsyncRead + std::marker::Unpin>,
    on_line: impl Fn(&str) -> &str,
) {
    log::debug!("Starting {name:?} reader task");

    let Some(stdout) = out else {
        log::error!("Failed to get {name:?} of process");
        return;
    };

    let mut line = String::new();
    let mut stdout = BufReader::new(stdout);

    while let Ok(n) = stdout.read_line(&mut line).await {
        _ = on_line(line[..n].trim());
        line.clear();
    }
}

async fn perform_server_command(command: ServerCommand) {
    match command {
        ServerCommand::Start { addr, port } => {
            let cdir = "/home/ae/.steam/steam/steamapps/common/Project Zomboid Dedicated Server";
            log::debug!("Changing current directory to {:?}", cdir);
            if let Err(err) = std::env::set_current_dir(cdir) {
                log::error!("{err}");
                return;
            }

            let listener = TcpListener::bind((addr, port)).await.unwrap();

            let (stop_tx, mut stop_rx) = watch::channel(false);

            let (start_tx, start_rx) = watch::channel(false);

            let pz = Arc::new(PZChild::new(Arc::new(start_tx)).await.unwrap());

            let mut set = JoinSet::new();

            loop {
                select! {
                    Ok(con) = listener.accept() => {
                        let stop = stop_tx.clone();
                        let pz = Arc::clone(&pz);
                        let start = start_rx.clone();

                        set.spawn(handle_connection(pz, stop, start.clone(), con));
                    }
                    _ = stop_rx.changed() => {
                        break;
                    }
                }
            }

            log::debug!("Waiting for all tasks");
            set.join_all().await;
            log::debug!("Exit successful");
        }
        ServerCommand::Stop { addr, port } => {
            let con = TcpStream::connect((addr, port)).await.unwrap();

            log::debug!("Waiting writable");
            con.writable().await.unwrap();

            let (reader, mut writer) = tokio::io::split(con);

            tokio::spawn(async move {
                log::debug!("Spawning reader task");
                let mut line = String::new();
                let mut reader = BufReader::new(reader);
                while let Ok(n) = reader.read_line(&mut line).await {
                    let line = &line[..n].trim();
                    println!("{line}");
                }
            });

            log::debug!("Writing quit");
            writer.write_all(b"quit\n").await.unwrap();
            writer.flush().await.unwrap();
            log::debug!("Wrote quit");
        }
    }
}

async fn handle_connection(
    pz: Arc<PZChild>,
    stop: watch::Sender<bool>,
    start: watch::Receiver<bool>,
    (mut stream, addr): (TcpStream, SocketAddr),
) {
    log::info!("New connection from {addr}");

    log::debug!("Waiting writable");
    stream.writable().await.unwrap();

    if !(*start.borrow()) {
        stream
            .write_all(b"Waiting for ProjectZomboid64 to start...\n")
            .await
            .unwrap();

        while !(*start.borrow()) {
            start.has_changed().unwrap();
            tokio::time::sleep(Duration::from_secs(1)).await;
        }

        stream.write_all(b"Ready!\n").await.unwrap();
    }

    log::info!("Waiting for messages from {addr}");
    let mut line = String::new();

    log::debug!("Waiting readable");
    stream.readable().await.unwrap();

    let (reader, writer) = tokio::io::split(stream);
    let mut reader = BufReader::new(reader);

    while let Ok(n) = reader.read_line(&mut line).await {
        if n == 0 {
            log::info!("Disconnected {addr}");
            break;
        }

        let l = &line[..n].trim();
        log::info!("{addr}: Got command: {l:?}");

        if ["stop", "exit", "quit", "q"].contains(l) {
            stop.send(true).unwrap();
            let (tx, rx) = oneshot::channel();
            pz.send("quit".to_string(), tx).await.unwrap();
            break;
        }

        line.clear();
    }
}
