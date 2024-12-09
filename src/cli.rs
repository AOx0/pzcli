use std::net::IpAddr;

use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader, BufWriter},
    net::TcpStream,
};

pub async fn prompt(addr: IpAddr, port: u16) {
    let stream = TcpStream::connect((addr, port)).await.unwrap();

    // log::debug!("Waiting for writable");
    // stream.writable().await.unwrap();

    let mut in_reader = BufReader::new(tokio::io::stdin());
    let mut line = String::new();

    let (reader, writer) = tokio::io::split(stream);
    let mut writer = BufWriter::new(writer);

    let handle = tokio::spawn(async move {
        let mut reader = BufReader::new(reader);
        let mut line = String::new();

        while let Ok(n) = reader.read_line(&mut line).await {
            if n == 0 {
                break;
            }

            if let Some((_, msg)) = line.trim().split_once("WARN: ") {
                log::warn!("{msg}");
            } else if let Some((_, msg)) = line.trim().split_once("ERROR: ") {
                log::error!("{msg}");
            } else if let Some((_, msg)) = line.trim().split_once("INFO: ") {
                log::info!("{msg}");
            } else {
                println!("{}", line.trim());
            }

            line.clear();
        }
    });

    loop {
        let n = in_reader.read_line(&mut line).await.unwrap();
        let l = &line[..n];

        if ["quit", "q", "beak"].contains(&l.trim()) {
            writer.write_all(b"quit\n").await.unwrap();
            writer.flush().await.unwrap();
            break;
        } else if l.trim() == "exit" {
            break;
        } else {
            writer.write_all(l.as_bytes()).await.unwrap();
            writer.flush().await.unwrap();
        }

        line.clear();
    }

    handle.await.unwrap();
}
