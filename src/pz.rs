use std::{process::Stdio, sync::Arc};

use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader, BufWriter},
    process::Command,
    sync::{broadcast, mpsc, watch},
    task::JoinSet,
};

pub struct PZChild {
    tx: mpsc::Sender<String>,
    lines: tokio::sync::broadcast::Sender<Line>,
    handlers: JoinSet<()>,
}

impl PZChild {
    pub async fn new(notifier: Arc<watch::Sender<bool>>) -> Result<PZChild, std::io::Error> {
        let (tx, mut rx) = mpsc::channel(100);
        let mut handlers = JoinSet::new();

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

        tokio::spawn({
            let notifier = notifier.clone();
            async move {
                let status = pz.wait().await.expect("child process encountered an error");
                notifier.send(true).unwrap();
                log::error!("child status was: {}", status);
            }
        });

        let (lines, _) = broadcast::channel(1500);

        handlers.spawn(handle_out_lines(
            "stderr",
            stderr,
            lines.clone(),
            Self::handle_log,
        ));
        handlers.spawn(handle_out_lines(
            "stdout",
            stdout,
            lines.clone(),
            move |line| {
                let msg = Self::handle_log(line);
                if msg.msg.contains("SERVER STARTED") {
                    notifier.send(true).unwrap();
                    log::info!("Server started!");
                }

                msg
            },
        ));

        handlers.spawn(async move {
            log::debug!("Starting Tokio task");
            let mut stdin = BufWriter::new(stdin);

            loop {
                let Some(msg): Option<String> = rx.recv().await else {
                    continue;
                };

                println!("Sending {msg:?}");
                stdin.write_all(msg.as_bytes()).await.unwrap();
                stdin.flush().await.unwrap();
            }
        });

        Ok(PZChild {
            tx,
            handlers,
            lines,
        })
    }

    pub async fn send(&self, cmd: String) -> Result<(), mpsc::error::SendError<String>> {
        self.tx.send(cmd).await
    }

    pub fn subscribe(&self) -> broadcast::Receiver<Line> {
        self.lines.subscribe()
    }

    fn handle_log(line: &str) -> Line {
        let Some((level, msg)) = line.split_once(":") else {
            return Line::raw(line);
        };
        let msg = msg.trim();

        let is_log = ["LOG", "ERROR", "WARN"]
            .iter()
            .any(|str| level.starts_with(str));

        if !is_log {
            return Line::raw(line);
        }

        let level = level.trim();
        let Some((from, msg)) = msg.split_once(",") else {
            return Line::log(msg, LineKind::Log);
        };
        let msg = msg.trim();
        let from = from.trim();

        let Some((_, msg)) = msg.split_once(">") else {
            return Line::log(msg, LineKind::Log);
        };
        let msg = msg.trim();

        let Some((n, msg)) = msg.split_once(">") else {
            return Line::log(msg, LineKind::Log);
        };
        let msg = msg.trim();

        let n = n.trim().replace(",", "");
        let n = n.parse::<u64>().unwrap_or_else(|err| {
            log::error!("Invalid log number {n}. Error: {err}");
            0
        });

        let msg = msg.trim();

        let level = match level.trim() {
            "LOG" => LineKind::Log,
            "WARN" => LineKind::Warn,
            "ERROR" => LineKind::Error,
            _ => return Line::raw(msg),
        };

        Line::log(&format!("{n:X} {from}: {msg}"), level)
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

#[derive(Debug, Clone)]
pub enum LineKind {
    Log,
    Error,
    Warn,
    Other,
}

#[derive(Debug, Clone)]
pub struct Line {
    pub kind: LineKind,
    pub msg: String,
}

impl Line {
    fn raw(msg: &str) -> Line {
        Line {
            kind: LineKind::Other,
            msg: msg.to_string(),
        }
    }

    fn log(msg: &str, kind: LineKind) -> Line {
        Line {
            kind,
            msg: msg.to_string(),
        }
    }
}

async fn handle_out_lines(
    name: &str,
    out: Option<impl tokio::io::AsyncRead + std::marker::Unpin>,
    sender: broadcast::Sender<Line>,
    on_line: impl Fn(&str) -> Line,
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

        let value = on_line(line[..n].trim());

        match value.kind {
            LineKind::Log => log::info!("{}", value.msg),
            LineKind::Error => log::error!("{}", value.msg),
            LineKind::Warn => log::warn!("{}", value.msg),
            LineKind::Other => println!("{}", value.msg),
        };

        let _ = sender.send(value); // We do not care if there are no listeners yet
        line.clear();
    }
}
