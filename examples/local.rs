use std::time::Duration;

use cpal::{
    self, SampleFormat, StreamConfig,
    traits::{DeviceTrait, HostTrait, StreamTrait},
};
use ringbuf::{HeapRb, traits::*};

use libhachimi::{audio_processing::AudioProcessor, constant::*, error};

fn main() -> anyhow::Result<()> {
    let mic_buf = HeapRb::<f32>::new(RB_SIZE);
    let (mut mic_prod, mic_cons) = mic_buf.split();

    let speaker_buf = HeapRb::<f32>::new(RB_SIZE);
    let (speaker_prod, mut speaker_cons) = speaker_buf.split();

    let processed_buf = HeapRb::<f32>::new(RB_SIZE);
    let (processed_prod, processed_cons) = processed_buf.split();

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

    let input_stream = input_device.build_input_stream(
        &input_config,
        move |data: &[f32], _| {
            mic_prod.push_slice(data);
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
        },
        |err| panic!("output error: {:?}", err),
        None,
    )?;

    // mic input audio process thread
    let _audio_process = std::thread::Builder::new()
        .name("Audio Pipeline Thread".to_owned())
        .spawn(move || {
            let mut filter =
                AudioProcessor::new(mic_cons, processed_cons, processed_prod, speaker_prod);
            loop {
                filter.process();
                // TODO: dynamic runtime modify
                std::thread::sleep(Duration::from_millis(16));
            }
        });

    input_stream.play()?;
    output_stream.play()?;

    println!("Audio system running. Channels: {}", output_channels);

    loop {
        println!("Running...");
        std::thread::sleep(Duration::from_secs(4));
    }
}
