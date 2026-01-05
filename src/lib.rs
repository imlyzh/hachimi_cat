use bytes::Bytes;
use hacore::FRAME20MS;
use tokio::sync::mpsc;

#[derive(Debug, Clone)]
pub enum DecodeCommand {
    DecodeNormal(Bytes),
    DecodeFEC(Bytes),
    DecodePLC,
}

#[derive(Debug, Clone)]
pub struct DecodedFrame {
    pub frame: Vec<f32>,
}

pub fn build_encoder(
    encoder_input: rtrb::Consumer<f32>,
    encoder_output: tokio::sync::broadcast::Sender<Bytes>,
) -> anyhow::Result<std::thread::JoinHandle<()>> {
    let encoder_process = std::thread::Builder::new()
        .name("Audio Encoder Thread".to_owned())
        .spawn(|| {
            if encode(encoder_input, encoder_output).is_err() {
                // cancellation
            }
        })?;
    Ok(encoder_process)
}

pub fn build_decoder(
    decoder_input: tokio::sync::mpsc::Receiver<DecodeCommand>,
    decoder_output: tokio::sync::mpsc::Sender<DecodedFrame>,
) -> anyhow::Result<std::thread::JoinHandle<()>> {
    let decode_process = std::thread::Builder::new()
        .name("Audio Encoder Thread".to_owned())
        .spawn(|| {
            if decode(decoder_input, decoder_output).is_err() {
                // cancellation
            }
        })?;
    Ok(decode_process)
}

pub fn build_mixer(
    mixer_input: tokio::sync::mpsc::Receiver<DecodedFrame>,
    mixer_output: rtrb::Producer<f32>,
) -> anyhow::Result<std::thread::JoinHandle<()>> {
    let decode_process = std::thread::Builder::new()
        .name("Audio Encoder Thread".to_owned())
        .spawn(|| {
            if mix(mixer_input, mixer_output).is_err() {
                // cancellation
            }
        })?;
    Ok(decode_process)
}

pub fn encode(
    mut encoder_input: rtrb::Consumer<f32>,
    encoder_output: tokio::sync::broadcast::Sender<Bytes>,
) -> anyhow::Result<()> {
    let mut encoder = opus::Encoder::new(48000, opus::Channels::Mono, opus::Application::Voip)?;
    encoder.set_bitrate(opus::Bitrate::Auto)?;
    encoder.set_vbr(true)?;
    encoder.set_inband_fec(true)?;
    // encoder.set_packet_loss_perc(0)?;

    let mut output = [0u8; 4096];

    loop {
        while let Ok(encoder_input) = encoder_input.read_chunk(FRAME20MS) {
            let encode_size = encoder.encode_float(encoder_input.as_slices().0, &mut output)?;
            let _ = encoder_output.send(Bytes::copy_from_slice(&output[..encode_size]));
        }
        std::thread::park();
    }
}

pub fn decode(
    decoder_input: tokio::sync::mpsc::Receiver<DecodeCommand>,
    decoder_output: tokio::sync::mpsc::Sender<DecodedFrame>,
) -> anyhow::Result<()> {
    let mut decoder = opus::Decoder::new(48000, opus::Channels::Mono)?;
    let mut decoder_input = decoder_input;

    let mut frame = [0f32; FRAME20MS];

    let decoder_output = decoder_output;

    loop {
        let decode_size = match decoder_input.blocking_recv() {
            Some(DecodeCommand::DecodeNormal(packet)) => {
                decoder.decode_float(&packet, &mut frame, false)
            }
            Some(DecodeCommand::DecodeFEC(packet)) => {
                decoder.decode_float(&packet, &mut frame, true)
            }
            Some(DecodeCommand::DecodePLC) => decoder.decode_float(&[], &mut frame, false),
            None => {
                return Ok(());
            }
        }?;
        if let Err(mpsc::error::TrySendError::Closed(_)) = decoder_output.try_send(DecodedFrame {
            frame: frame[..decode_size].to_vec(),
        }) {
            // TODO: cancel
            return Ok(());
        }
    }
}

pub fn mix(
    mixer_input: tokio::sync::mpsc::Receiver<DecodedFrame>,
    mixer_output: rtrb::Producer<f32>,
) -> anyhow::Result<()> {
    let mut mixer_input = mixer_input;
    let mut mixer_output = mixer_output;

    loop {
        if let Ok(mut mixer_output) = mixer_output.write_chunk(FRAME20MS) {
            match mixer_input.try_recv() {
                Ok(frame) => {
                    mixer_output.as_mut_slices().0.copy_from_slice(&frame.frame);
                }
                Err(mpsc::error::TryRecvError::Empty) => {
                    mixer_output
                        .as_mut_slices()
                        .0
                        .copy_from_slice(&[0f32; FRAME20MS]);
                }
                Err(mpsc::error::TryRecvError::Disconnected) => {
                    return Ok(());
                }
            }
        }
        std::thread::park();
    }
}
