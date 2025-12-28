use std::{sync::Arc, time::Duration};

use cpal::{
    self, SampleFormat, StreamConfig,
    traits::{DeviceTrait, HostTrait, StreamTrait},
};
use ringbuf::{HeapRb, traits::*};

use libhachimi::{audio_processing::AudioProcessor, constant::*, error};

fn main() -> anyhow::Result<()> {
    // config

    let host = cpal::default_host();

    let input_device = host
        .default_input_device()
        .ok_or(error::Error::InputDeviceInitError)?;

    let mut supported_input_configs = input_device.supported_input_configs()?;
    let input_config = supported_input_configs
        .find(|config| {
            config.sample_format() == SampleFormat::F32
                && config.min_sample_rate() <= SAMPLE_RATE
                && config.max_sample_rate() >= SAMPLE_RATE
                && config.channels() == 1
        })
        .map(|config| config.with_sample_rate(SAMPLE_RATE))
        .ok_or(error::Error::UnsupportedInputSampleFormat)?;

    let input_config: StreamConfig = input_config.into();

    let output_device = host
        .default_output_device()
        .ok_or(error::Error::OutputDeviceInitError)?;

    let mut supported_output_configs = output_device.supported_output_configs()?;
    let output_config = supported_output_configs
        .find(|config| {
            config.sample_format() == SampleFormat::F32
                && config.min_sample_rate() <= SAMPLE_RATE
                && config.max_sample_rate() >= SAMPLE_RATE
                && config.channels() <= 2
        })
        .map(|config| config.with_sample_rate(SAMPLE_RATE))
        .ok_or(error::Error::UnsupportedOutputSampleFormat)?;

    let output_config: StreamConfig = output_config.into();

    let output_channels = output_config.channels as usize;

    // buffer init

    let mic_buf = HeapRb::<f32>::new(RB_SIZE);
    let (mut mic_prod, mic_cons) = mic_buf.split();

    let speaker_buf = HeapRb::<f32>::new(RB_SIZE);
    let (speaker_prod, mut speaker_cons) = speaker_buf.split();

    let ap_to_encoder = HeapRb::<f32>::new(RB_SIZE);
    let (ap_mic_output, mut encoder_input) = ap_to_encoder.split();

    let decoder_to_ap = HeapRb::<f32>::new(RB_SIZE);
    let (mut decoder_output, ap_ref_input) = decoder_to_ap.split();

    let (encoder_output, decoder_input) = std::sync::mpsc::sync_channel(4);
    // let decoder_input = Arc::new(decoder_input);

    // start threads

    let audio_process = std::thread::Builder::new()
        .name("Audio Pipeline Thread".to_owned())
        .spawn(move || {
            let mut filter =
                AudioProcessor::new(mic_cons, ap_ref_input, ap_mic_output, speaker_prod);
            loop {
                filter.process();
                // TODO: dynamic runtime modify
                std::thread::park_timeout(Duration::from_millis(10));
            }
        })
        .unwrap();
    let audio_process = Arc::new(audio_process);
    let audio_process_0 = audio_process.clone();
    let audio_process_1 = audio_process.clone();
    let audio_process_2 = audio_process.clone();
    let audio_process_3 = audio_process.clone();

    let input_stream = input_device.build_input_stream(
        &input_config,
        move |data: &[f32], _| {
            mic_prod.push_slice(data);
            audio_process_0.thread().unpark();
        },
        |err| panic!("input error: {:?}", err),
        None,
    )?;

    let output_stream = output_device.build_output_stream(
        &output_config,
        move |output: &mut [f32], _| {
            for frame in output.chunks_exact_mut(output_channels) {
                if let Some(sample) = speaker_cons.try_pop() {
                    for channel_sample in frame.iter_mut() {
                        *channel_sample = sample;
                    }
                } else {
                    for channel_sample in frame.iter_mut() {
                        *channel_sample = 0.0;
                    }
                }
            }
            audio_process_1.thread().unpark();
        },
        |err| panic!("output error: {:?}", err),
        None,
    )?;

    let encoder_process = std::thread::Builder::new()
        .name("Audio Encoder Thread".to_owned())
        .spawn(move || {
            let mut encoder =
                opus::Encoder::new(SAMPLE_RATE, opus::Channels::Mono, opus::Application::Voip)
                    .unwrap();
            encoder.set_bitrate(opus::Bitrate::Auto).unwrap();

            let mut frame = [0f32; FRAME_SIZE];
            let mut output = [0u8; 4096];

            loop {
                while encoder_input.occupied_len() >= FRAME_SIZE {
                    encoder_input.pop_slice(&mut frame);
                    let encode_size = encoder.encode_float(&frame, &mut output).unwrap();
                    encoder_output.send(output[..encode_size].to_vec()).unwrap();
                    audio_process_2.clone().thread().unpark();
                }
                std::thread::park_timeout(Duration::from_millis(10));
            }
        })
        .unwrap();
    let encoder_process = Arc::new(encoder_process);

    let decoder_process = std::thread::Builder::new()
        .name("Audio Encoder Thread".to_owned())
        .spawn(move || {
            let mut decoder = opus::Decoder::new(SAMPLE_RATE, opus::Channels::Mono).unwrap();

            let mut frame = [0f32; FRAME_SIZE];

            loop {
                if decoder_output.vacant_len() >= FRAME_SIZE {
                    let packet = decoder_input.recv().unwrap();
                    let decode_size = decoder.decode_float(&packet, &mut frame, false).unwrap();
                    decoder_output.push_slice(&frame[..decode_size]);
                    audio_process_3.thread().unpark();
                    // FIXME: move to network thread
                    encoder_process.thread().unpark();
                }
                std::thread::park_timeout(Duration::from_millis(10));
            }
        })
        .unwrap();
    let _decoder_process = Arc::new(decoder_process);

    input_stream.play()?;
    output_stream.play()?;

    println!("Audio system running. Channels: {}", output_channels);

    loop {
        println!("Running...");
        std::thread::sleep(Duration::from_secs(4));
    }
}
