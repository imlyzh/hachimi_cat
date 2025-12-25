use std::time::Duration;

use cpal::{
    self, SampleFormat, StreamConfig,
    traits::{DeviceTrait, HostTrait, StreamTrait},
};
use ringbuf::{HeapRb, traits::*};

use hachimi_cat::{audio_processing::audio_processing, constant::*, error};

fn main() -> anyhow::Result<()> {
    let mic_buf = HeapRb::<f32>::new(RB_SIZE);
    let (mut mic_prod, mic_cons) = mic_buf.split();

    let far_end_buf = HeapRb::<f32>::new(RB_SIZE);
    let (mut far_end_prod, far_end_cons) = far_end_buf.split();

    let processed_buf = HeapRb::<f32>::new(RB_SIZE);
    let (processed_prod, mut processed_cons) = processed_buf.split();

    let host = cpal::default_host();

    // let config = InitializationConfig {
    //     num_capture_channels: 1,
    //     num_render_channels: 1,
    //     sample_rate: Some(SAMPLE_RATE),
    //     ..InitializationConfig::default()
    // };
    // let mut audio_processor = Processor::new(&config)?;
    // let config = Config {
    //     echo_cancellation: Some(EchoCancellation {
    //         suppression_level: EchoCancellationSuppressionLevel::High,
    //         enable_delay_agnostic: true,
    //         enable_extended_filter: false,
    //         stream_delay_ms: None,
    //     }),
    //     noise_suppression: Some(NoiseSuppression {
    //         suppression_level: NoiseSuppressionLevel::High,
    //     }),
    //     gain_control: Some(GainControl {
    //         mode: GainControlMode::AdaptiveDigital,
    //         target_level_dbfs: 3,
    //         compression_gain_db: 0,
    //         enable_limiter: true,
    //     }),
    //     ..Config::default()
    // };
    // audio_processor.set_config(config);

    let input_device = host
        .default_input_device()
        .ok_or(error::Error::InputDeviceInitError)?;
    let mut supported_input_configs = input_device.supported_input_configs()?;
    let input_config = supported_input_configs
        .find(|config| {
            config.sample_format() == SampleFormat::F32
                && config.min_sample_rate() <= TARGET_RATE
                && config.max_sample_rate() >= TARGET_RATE
                && config.channels() <= 1
        })
        .map(|config| config.with_sample_rate(TARGET_RATE))
        .ok_or(error::Error::UnsupportedSampleFormat)?;

    let input_config: StreamConfig = input_config.into();

    let output_device = host
        .default_output_device()
        .ok_or(error::Error::OutputDeviceInitError)?;
    let mut supported_output_configs = output_device.supported_output_configs()?;
    let output_config = supported_output_configs
        .find(|config| {
            config.sample_format() == SampleFormat::F32
                && config.min_sample_rate() <= TARGET_RATE
                && config.max_sample_rate() >= TARGET_RATE
                && config.channels() <= 2
        })
        .map(|config| config.with_sample_rate(TARGET_RATE))
        .ok_or(error::Error::UnsupportedSampleFormat)?;

    let output_config: StreamConfig = output_config.into();

    let input_stream = input_device.build_input_stream(
        &input_config,
        move |data, _| {
            mic_prod.push_slice(data);
        },
        |err| panic!("error: {:?}", err),
        None,
    )?;

    let output_stream = output_device.build_output_stream(
        &output_config,
        move |output, _| {
            for frame in output.chunks_exact_mut(2) {
                if let Some(sample) = processed_cons.try_pop() {
                    frame[0] = sample;
                    frame[1] = sample;
                    far_end_prod.push_slice(&[sample]);
                } else {
                    frame[0] = 0.0;
                    frame[1] = 0.0;
                }
            }
        },
        |err| panic!("error: {:?}", err),
        None,
    )?;

    // mic input audio process thread
    let audio_process =
        std::thread::spawn(move || audio_processing(mic_cons, far_end_cons, processed_prod));

    input_stream.play()?;
    output_stream.play()?;
    // audio_process.join().unwrap();

    loop {
        println!("test");
        std::thread::sleep(Duration::from_secs(5));
    }
}
