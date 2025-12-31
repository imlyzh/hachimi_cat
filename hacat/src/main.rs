use std::str::FromStr;

use bytes::Bytes;
use clap::{Parser, Subcommand};
use hacore::AudioEngine;
use iroh::{Endpoint, EndpointId, endpoint::Connection};
use ringbuf::{HeapRb, traits::Split};
use tokio::task::JoinHandle;

#[derive(Parser)]
#[command(name = "hacat")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    Listen,
    Call { id: String },
}

const ALPN: &[u8] = b"hacat/opus/1.0";

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    let mdns = iroh::discovery::mdns::MdnsDiscovery::builder();
    let dht = iroh::discovery::pkarr::dht::DhtDiscovery::builder();

    let alpns = vec![ALPN.to_vec()];

    let mut running_services = vec![];

    match cli.command {
        Commands::Listen => {
            let endpoint = Endpoint::builder()
                .discovery(mdns)
                .discovery(dht)
                .alpns(alpns)
                .bind()
                .await?;
            let local_id = endpoint.id();
            println!("local id: {}", local_id);

            while let Some(incoming) = endpoint.accept().await {
                let connecting = incoming.accept()?;
                let connection = connecting.await?;

                let audio_services = AudioServices::build(connection).await?;
                running_services.push(audio_services);
            }
        }
        Commands::Call { id } => {
            let endpoint = Endpoint::builder()
                .discovery(mdns)
                .discovery(dht)
                .alpns(alpns)
                .bind()
                .await?;
            let connection = endpoint.connect(EndpointId::from_str(&id)?, ALPN).await?;

            let audio_services = AudioServices::build(connection).await?;
            running_services.push(audio_services);
        }
    }

    tokio::signal::ctrl_c().await?;

    for service in running_services {
        // TODO: safety close connection
    }

    println!("Shutting down.");
    Ok(())
}

pub struct AudioServices {
    pub ae: AudioEngine,
    pub connection: Connection,
    pub sender_thread: JoinHandle<()>,
    pub reciver_thread: JoinHandle<()>,
}

impl AudioServices {
    async fn build(connection: Connection) -> anyhow::Result<Self> {
        let (local_prod, mut local_cons) = tokio::sync::mpsc::channel(100);
        let remote_buf = HeapRb::new(4);
        let (remote_prod, remote_cons) = remote_buf.split();

        let conn_for_send = connection.clone();
        let conn_for_recv = connection.clone();

        let sender_thread = tokio::task::spawn(async move {
            while let Some(frame) = local_cons.recv().await {
                // TODO: encoding rtp frame
                conn_for_send.send_datagram(Bytes::from(frame)).unwrap();
            }
        });

        let reciver_thread = tokio::task::spawn(async move {
            loop {
                let read_datagram = conn_for_recv.read_datagram().await.unwrap();
                // TODO: decoding rtp frame
                // TODO: jitter
                // TODO: send to remote_prod
            }
        });

        Ok(AudioServices {
            ae: AudioEngine::build(local_prod, remote_cons)?,
            connection,
            sender_thread,
            reciver_thread,
        })
    }
}
