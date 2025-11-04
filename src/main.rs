use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4};
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result, bail};
use clap::Parser;
use reqwest::Client;
use tokio::net::UdpSocket;
use tokio::signal::ctrl_c;
use tokio::sync::mpsc::unbounded_channel;

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
    let max_retries = 3;
    let backoff_ms = 200;

    let socket = UdpSocket::bind("0.0.0.0:0").await?;

    let mut packet = Vec::new();
    packet.push(0x00);
    packet.extend_from_slice(name.as_bytes());
    packet.push(0xFF);

    let addr = SocketAddr::V4(SocketAddrV4::new(*rendezvous, 4200));

    for _ in 1..=max_retries {
        socket.send_to(&packet, addr).await?;
        tokio::time::sleep(Duration::from_millis(backoff_ms)).await;
    }
    Ok(socket)
}

pub async fn get_peer_address(rendezvous: &Ipv4Addr, peer: &str) -> Result<SocketAddr> {
    let client = Client::new();

    let http_timeout = 10;
    let max_retries = 5;
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
    let sock_pointer = Arc::new(register(&cli.rendezvous, &cli.name).await?);
    let peer = get_peer_address(&cli.rendezvous, &cli.peer).await?;

    let (tx, mut rx) = unbounded_channel();
    let tx_input = tx.clone();
    std::thread::spawn(move || {
        let mut line = String::new();
        loop {
            line.clear();
            if std::io::stdin().read_line(&mut line).is_ok() {
                let msg = line.trim().to_string();
                if msg.is_empty() {
                    continue;
                }
                if tx_input.send(msg).is_err() {
                    break;
                }
            } else {
                break;
            }
        }
    });

    println!("chat with {} — type and press Enter", peer);
    let mut buf = vec![0u8; 2048];

    let chat_sock = Arc::clone(&sock_pointer);
    tokio::spawn(async move {
        loop {
            tokio::select! {
                res = chat_sock.recv_from(&mut buf) => {
                    if let Ok((len, from)) = res {
                        if len == 0 { continue; }
                        let text = String::from_utf8_lossy(&buf[..len]);
                        println!("[{from}] {text}");
                    }
                }
                Some(line) = rx.recv() => {
                    if chat_sock.send_to(line.as_bytes(), peer).await.is_ok() {
                        println!("Sent: {line}");
                    }
                }
            }
        }
    });

    let punch_sock = Arc::clone(&sock_pointer);
    for _ in 0..100 {
        punch_sock.send_to(b"punch", peer).await?;
        tokio::time::sleep(Duration::from_millis(25)).await;
    }

    println!("Client running. Press Ctrl+C to exit.");
    ctrl_c().await?;
    println!("Shutting down...");
    Ok(())
}
