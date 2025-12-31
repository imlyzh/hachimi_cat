pub mod webrtc_audio_processing;

use std::{sync::Arc, thread::JoinHandle};

use cpal::{
    self, SampleFormat, Stream, StreamConfig,
    traits::{DeviceTrait, HostTrait, StreamTrait},
};
use ringbuf::{HeapCons, HeapRb, traits::*};

// use libhachimi::audio_processing::AudioProcessor;
use libhachimi::{constant::*, error};

use crate::webrtc_audio_processing::{AudioProcessor, FRAME20MS};

pub struct AudioEngine {
    input_stream: Stream,
    output_stream: Stream,
    pub decode_process: Arc<JoinHandle<()>>,
}

#[derive(Debug, Clone)]
pub enum DecodeCommand {
    DecodeNormal(Vec<u8>),
    DecodeFEC(Vec<u8>),
    DecodePLC,
}

impl AudioEngine {
    pub fn build(
        encoder_output: tokio::sync::mpsc::Sender<Vec<u8>>,
        decoder_input: HeapCons<DecodeCommand>,
    ) -> anyhow::Result<AudioEngine> {
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

        let mic_buf = HeapRb::<f32>::new(FRAME20MS * 2);
        let (mut mic_prod, mic_cons) = mic_buf.split();

        let speaker_buf = HeapRb::<f32>::new(FRAME20MS * 2);
        let (speaker_prod, mut speaker_cons) = speaker_buf.split();

        let ap_to_encoder = HeapRb::<f32>::new(FRAME20MS * 2);
        let (ap_mic_output, mut encoder_input) = ap_to_encoder.split();

        let decoder_to_ap = HeapRb::<f32>::new(FRAME20MS * 2);
        let (mut decoder_output, ap_ref_input) = decoder_to_ap.split();

        // start threads

        let encode_process = std::thread::Builder::new()
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
                        let _ = encoder_output.try_send(output[..encode_size].to_vec());
                    }
                    std::thread::park();
                }
            })
            .unwrap();

        let audio_process = std::thread::Builder::new()
            .name("Audio Pipeline Thread".to_owned())
            .spawn(move || {
                let mut ap = AudioProcessor::build().unwrap();
                let mut mic_cons = mic_cons;
                let mut ap_ref_input = ap_ref_input;
                let mut ap_mic_output = ap_mic_output;
                let mut speaker_prod = speaker_prod;
                loop {
                    ap.process(
                        &mut mic_cons,
                        &mut ap_ref_input,
                        &mut ap_mic_output,
                        &mut speaker_prod,
                    );
                    encode_process.thread().unpark();
                    std::thread::park();
                }
            })
            .unwrap();
        let audio_process = Arc::new(audio_process);
        let audio_process_0 = audio_process.clone();
        let audio_process_1 = audio_process.clone();
        // let audio_process_2 = audio_process.clone();

        let decode_process = std::thread::Builder::new()
            .name("Audio Encoder Thread".to_owned())
            .spawn(move || {
                let mut decoder = opus::Decoder::new(SAMPLE_RATE, opus::Channels::Mono).unwrap();
                let mut decoder_input = decoder_input;

                let mut frame = [0f32; FRAME_SIZE];

                loop {
                    if decoder_output.vacant_len() >= FRAME_SIZE {
                        let decode_size = match decoder_input.try_pop() {
                            Some(DecodeCommand::DecodeNormal(packet)) => {
                                decoder.decode_float(&packet, &mut frame, false).unwrap()
                            }
                            Some(DecodeCommand::DecodeFEC(packet)) => {
                                decoder.decode_float(&packet, &mut frame, true).unwrap()
                            }
                            Some(DecodeCommand::DecodePLC) | None => {
                                decoder.decode_float(&[], &mut frame, false).unwrap()
                            }
                        };
                        decoder_output.push_slice(&frame[..decode_size]);
                        audio_process_0.thread().unpark();
                    }
                    std::thread::park();
                }
            })
            .unwrap();
        let decode_process = Arc::new(decode_process);

        let input_stream = input_device.build_input_stream(
            &input_config,
            move |data: &[f32], _| {
                mic_prod.push_slice(data);
                audio_process_1.thread().unpark();
            },
            |err| panic!("input error: {:?}", err),
            None,
        )?;

        let output_stream = output_device.build_output_stream(
            &output_config,
            move |output: &mut [f32], _| {
                // 只能象征性催一下
                // audio_process_2.thread().unpark();
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
            },
            |err| panic!("output error: {:?}", err),
            None,
        )?;

        println!("Audio system running. Channels: {}", output_channels);

        Ok(AudioEngine {
            decode_process,
            input_stream,
            output_stream,
        })
    }

    pub fn notify_decoder(&self) {
        self.decode_process.thread().unpark();
    }

    pub fn play(&self) -> anyhow::Result<()> {
        self.input_stream.play()?;
        self.output_stream.play()?;
        Ok(())
    }

    pub fn pause(&self) -> anyhow::Result<()> {
        self.input_stream.pause()?;
        self.output_stream.pause()?;
        Ok(())
    }

    pub fn enable_mic(&self) -> anyhow::Result<()> {
        self.input_stream.play()?;
        Ok(())
    }

    pub fn disable_mic(&self) -> anyhow::Result<()> {
        self.input_stream.pause()?;
        Ok(())
    }
}
