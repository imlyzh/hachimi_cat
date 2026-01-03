use std::{str::FromStr, sync::Arc};

use bytes::Bytes;
use clap::{Parser, Subcommand};
#[cfg(target_vendor = "apple")]
use hacore::EngineBuilder;
use hacore::{AudioEngine, DecodeCommand, FRAME10MS};
use iroh::{Endpoint, EndpointId, endpoint::Connection};
use ringbuf::{
    HeapRb,
    traits::{Producer, Split},
};
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

                let audio_services = AudioServices::build(connection)?;
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

            let audio_services = AudioServices::build(connection)?;
            running_services.push(audio_services);
        }
    }

    tokio::signal::ctrl_c().await?;

    // for service in running_services {
    // TODO: safety close connection
    // service.connection.close()
    // }

    println!("Shutting down.");
    Ok(())
}

pub struct AudioServices {
    pub ae: Arc<dyn AudioEngine>,
    pub connection: Connection,
    pub sender_thread: JoinHandle<()>,
    pub reciver_thread: JoinHandle<()>,
}

impl AudioServices {
    fn build(connection: Connection) -> anyhow::Result<Self> {
        let (local_prod, mut local_cons) = tokio::sync::mpsc::channel(100);
        let (mut remote_prod, remote_cons) = HeapRb::new(FRAME10MS * 10).split();

        #[cfg(not(target_vendor = "apple"))]
        let ae: Arc<dyn AudioEngine> =
            hacore::default_audio_engine::DefaultAudioEngine::build(local_prod, remote_cons)?;
        #[cfg(target_vendor = "apple")]
        let ae: Arc<dyn AudioEngine> =
            hacore::apple_platform_audio_engine::ApplePlatformAudioEngine::build(
                local_prod,
                remote_cons,
            )?;

        let decoder_thread = ae.get_decoder_thread();

        let conn_for_send = connection.clone();
        let conn_for_recv = connection.clone();

        let sender_thread = tokio::task::spawn(async move {
            while let Some(frame) = local_cons.recv().await {
                // TODO: encoding rtp frame
                conn_for_send.send_datagram(Bytes::from(frame)).unwrap();
            }
        });

        let reciver_thread = tokio::task::spawn(async move {
            // let ae1 = ae1.clone();
            while let Ok(frame) = conn_for_recv.read_datagram().await {
                // TODO: decoding rtp frame
                // TODO: jitter
                let _ = remote_prod.try_push(DecodeCommand::DecodeNormal(frame.to_vec()));
                decoder_thread.thread().unpark();
            }
        });

        Ok(AudioServices {
            ae,
            connection,
            sender_thread,
            reciver_thread,
        })
    }
}
