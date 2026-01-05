use std::collections::HashMap;
use std::{str::FromStr, sync::Arc};

use bytes::Bytes;
use clap::{Parser, Subcommand};
use hachimi_cat::{DecodeCommand, DecodedFrame, build_decoder, build_encoder, build_mixer};
use hacore::AudioEngine;
use hacore::{EngineBuilder, FRAME20MS};
use iroh::{Endpoint, EndpointId, endpoint::Connection};
use tokio::sync::broadcast;
use tokio::sync::mpsc;

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

const ALPN: &[u8] = b"hacat/opus/1";

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    let mdns = iroh::discovery::mdns::MdnsDiscovery::builder();
    let dht = iroh::discovery::pkarr::dht::DhtDiscovery::builder();

    let alpns = vec![ALPN.to_vec()];

    let mut audio_services = AudioServices::new()?;

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

                audio_services.add_connection(connection)?;
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

            audio_services.add_connection(connection)?;
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
    send_data_cons: broadcast::Receiver<Bytes>,
    decode_frame_prod: mpsc::Sender<DecodedFrame>,
    pub mixer_thread: Arc<std::thread::JoinHandle<()>>,
    connect_pair: HashMap<EndpointId, ConnectPair>,
}

pub struct ConnectPair {
    pub connection: Connection,
    pub sender_thread: tokio::task::JoinHandle<()>,
    pub reciver_thread: tokio::task::JoinHandle<()>,
    pub decoder_thread: std::thread::JoinHandle<()>,
}

impl AudioServices {
    fn new() -> anyhow::Result<Self> {
        let (ae_mic_output, encoder_input) = rtrb::RingBuffer::new(FRAME20MS * 4);
        let (mixer_output, ae_ref_input) = rtrb::RingBuffer::new(FRAME20MS * 4);

        let (send_data_prod, send_data_cons) = tokio::sync::broadcast::channel(2);
        let encoder_thread = build_encoder(encoder_input, send_data_prod)?;

        let (decode_frame_prod, mixer_input) = tokio::sync::mpsc::channel(2);
        let mixer_thread = build_mixer(mixer_input, mixer_output)?;
        let mixer_thread = Arc::new(mixer_thread);

        #[cfg(not(target_vendor = "apple"))]
        let ae: Arc<dyn AudioEngine> = hacore::default_audio_engine::DefaultAudioEngine::build(
            ae_mic_output,
            ae_ref_input,
            encoder_thread,
            mixer_thread.clone(),
        )?;
        #[cfg(target_vendor = "apple")]
        let ae: Arc<dyn AudioEngine> =
            hacore::apple_platform_audio_engine::ApplePlatformAudioEngine::build(
                ae_mic_output,
                ae_ref_input,
                encoder_thread,
                mixer_thread.clone(),
            )?;

        Ok(AudioServices {
            ae,
            connect_pair: HashMap::default(),
            send_data_cons,
            decode_frame_prod,
            mixer_thread,
        })
    }

    pub fn add_connection(&mut self, connection: Connection) -> anyhow::Result<()> {
        let conn_for_send = connection.clone();
        let conn_for_recv = connection.clone();

        let (recv_data_prod, recv_data_cons) = tokio::sync::mpsc::channel(2);
        let decoder_thread = build_decoder(recv_data_cons, self.decode_frame_prod.clone())?;
        let mut send_data_cons = self.send_data_cons.resubscribe();

        let sender_thread = tokio::task::spawn(async move {
            while let Ok(frame) = send_data_cons.recv().await {
                // TODO: encoding rtp frame
                if conn_for_send.send_datagram(frame).is_err() {
                    // TODO: cancellization
                    return;
                }
            }
        });

        let reciver_thread = tokio::task::spawn(async move {
            // let ae1 = ae1.clone();
            while let Ok(frame) = conn_for_recv.read_datagram().await {
                // TODO: decoding rtp frame
                // TODO: jitter
                let _ = recv_data_prod
                    .send(DecodeCommand::DecodeNormal(frame))
                    .await;
            }
        });

        self.connect_pair.insert(
            connection.remote_id(),
            ConnectPair {
                connection,
                sender_thread,
                reciver_thread,
                decoder_thread,
            },
        );
        Ok(())
    }
}
