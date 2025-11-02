use std::net::{Ipv4Addr, SocketAddr};
use std::time::Duration;

use anyhow::{Context, Result, bail};
use clap::{Parser, arg};
use reqwest::Client;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::net::UdpSocket;

#[derive(Parser)]
struct Cli {
    #[arg(long, default_value = "one")]
    name: String,
    /// Peer name to connect to
    #[arg(long, default_value = "other")]
    peer: String,
    /// Rendezvous server IP (should be a public IP)
    #[arg(long, default_value = "45.151.30.139")]
    rendezvous: Ipv4Addr,
}

async fn register(rendezvous: &Ipv4Addr, name: &str) -> Result<UdpSocket> {
    let mut retries = 0;
    let max_retries = 10;
    let mut backoff_ms = 100;

    let socket = UdpSocket::bind("0.0.0.0:0").await?;

    let mut packet = Vec::new();
    packet.push(0x00);
    packet.extend_from_slice(name.as_bytes());
    packet.push(0xFF);

    while retries < max_retries {
        socket
            .send_to(&packet, format!("{}:4200", rendezvous))
            .await?;
        retries += 1;
        tokio::time::sleep(Duration::from_millis(backoff_ms)).await;
        backoff_ms += 100;
    }

    Ok(socket)
}

pub async fn get_peer_address(rendezvous: &Ipv4Addr, peer: &str) -> Result<SocketAddr> {
    let client = Client::new();

    let http_timeout = 10;
    let max_retries = 3;
    let mut backoff = Duration::from_millis(400);

    for attempt in 1..=max_retries {
        let url = format!(
            "http://{}:8080/api/wait/{}?timeout={}",
            rendezvous, peer, http_timeout
        );

        let response = match client.get(&url).send().await {
            Ok(r) => r,
            Err(_) => {
                if attempt < max_retries {
                    tokio::time::sleep(backoff).await;
                    backoff = (backoff * 2).min(Duration::from_secs(5));
                    continue;
                } else {
                    bail!(
                        "network error while querying rendezvous (attempt {attempt}/{max_retries})"
                    );
                }
            }
        };

        match response.status().as_u16() {
            200 => {
                let body = response.text().await.context("read wait body")?;
                let addr: SocketAddr = body
                    .trim()
                    .parse()
                    .with_context(|| format!("parse peer address from '{}'", body.trim()))?;
                return Ok(addr);
            }
            404 => {
                if attempt < max_retries {
                    tokio::time::sleep(Duration::from_millis(600)).await;
                    backoff = Duration::from_millis(400);
                    continue;
                } else {
                    bail!("peer '{}' not found at rendezvous (404)", peer);
                }
            }
            other => bail!("unexpected HTTP status {other} from rendezvous"),
        }
    }

    bail!("failed to resolve peer after {max_retries} attempts");
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    let socket = register(&cli.rendezvous, &cli.name).await?;
    let peer = get_peer_address(&cli.rendezvous, &cli.peer).await?;

    for _ in 0..5 {
        tokio::time::sleep(Duration::from_millis(600)).await;
        socket.send_to(&[0], peer).await?;
    }

    let stdin = tokio::io::stdin();
    let mut lines = BufReader::new(stdin).lines();
    let mut buf = vec![0u8; 1024];

    println!("chat with {} — type and press Enter", peer);

    loop {
        tokio::select! {
            res = socket.recv_from(&mut buf) => {
                            let (len, from) = res?;
                            if len == 0 { continue; }
                            match std::str::from_utf8(&buf[..len]) {
                                Ok(text) => println!("[{from}] {text}"),
                                Err(_)   => println!("[{from}] <{len} bytes>"),
                            }
                        }

            maybe_line = lines.next_line() => {
                match maybe_line? {
                    Some(line) => {
                        if !line.is_empty() {
                            socket.send_to(line.as_bytes(), peer).await?;
                        }
                    }
                    None => {
                        break;
                    }
                }
            }
        }
    }
    Ok(())
}
