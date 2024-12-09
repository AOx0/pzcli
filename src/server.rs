use std::{net::SocketAddr, sync::Arc};

use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    net::{TcpListener, TcpStream},
    select,
    sync::{oneshot, watch},
    task::JoinSet,
};

use crate::args::ServerCommand;
use crate::pz::PZChild;

pub async fn perform(command: ServerCommand) {
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
                    if n == 0 {
                        log::info!("Connection dropped");
                        break;
                    }

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
            tokio::time::sleep(std::time::Duration::from_secs(1)).await;
        }

        stream.write_all(b"Ready!\n").await.unwrap();
    }

    log::info!("Waiting for messages from {addr}");
    let mut line = String::new();

    // log::debug!("Waiting readable");
    // stream.readable().await.unwrap();

    let (reader, writer) = tokio::io::split(stream);
    let mut reader = BufReader::new(reader);

    while let Ok(n) = reader.read_line(&mut line).await {
        if n == 0 {
            log::info!("Disconnected {addr}");
            break;
        }

        let l = &line[..n];
        log::info!("{addr}: Got command: {l:?}");

        let (tx, rx) = oneshot::channel();
        pz.send(l.to_string(), tx).await.unwrap();

        if ["stop", "exit", "quit", "q"].contains(&l.trim()) {
            stop.send(true).unwrap();
            break;
        }

        line.clear();
    }
}
