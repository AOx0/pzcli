use std::{process::Stdio, sync::Arc};

use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader, BufWriter},
    process::Command,
    sync::{mpsc, oneshot, watch},
    task::JoinSet,
};

pub struct PZChild {
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

        set.spawn(handle_out_lines("stderr", stderr, Self::handle_log));
        set.spawn(handle_out_lines("stdout", stdout, move |line| {
            let msg = Self::handle_log(line);
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
            let msg = msg.trim();
            let from = from.trim();

            let Some((_, msg)) = msg.split_once(">") else {
                log::info!("{msg}");
                return msg;
            };
            let msg = msg.trim();

            let Some((n, msg)) = msg.split_once(">") else {
                log::info!("{msg}");
                return msg;
            };
            let msg = msg.trim();

            let n = n.trim().replace(",", "");
            let n = n.parse::<u64>().unwrap_or_else(|err| {
                log::error!("Invalid log number {n}. Error: {err}");
                0
            });

            let msg = msg.trim();
            match level.trim() {
                "LOG" => log::info!("{n:x} {from}: {msg}"),
                "WARN" => log::warn!("{n:x} {from}: {msg}"),
                "ERROR" => log::error!("{n:x} {from}: {msg}"),
                _ => log::debug!("{msg}"),
            }

            msg
        } else {
            println!("{line}");
            line
        }
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
        if n == 0 {
            log::info!("ProjectZomboid64 stopped sending lines");
            break;
        }

        _ = on_line(line[..n].trim());
        line.clear();
    }
}
