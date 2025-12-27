use std::time::Duration;

use cpal::{
    self, SampleFormat, StreamConfig,
    traits::{DeviceTrait, HostTrait, StreamTrait},
};
use ringbuf::{HeapRb, traits::*};

use hachimi_cat::{audio_processing::AudioProcessor, constant::*, error};

fn main() -> anyhow::Result<()> {
    // ---------------- RingBuffer 初始化 ----------------
    let mic_buf = HeapRb::<f32>::new(RB_SIZE);
    let (mut mic_prod, mic_cons) = mic_buf.split();

    let speaker_buf = HeapRb::<f32>::new(RB_SIZE);
    let (speaker_prod, mut speaker_cons) = speaker_buf.split();

    let processed_buf = HeapRb::<f32>::new(RB_SIZE);
    let (processed_prod, processed_cons) = processed_buf.split();

    let host = cpal::default_host();

    // ---------------- Input 配置 ----------------

    let input_device = host
        .default_input_device()
        .ok_or(error::Error::InputDeviceInitError)?;

    let mut supported_input_configs = input_device.supported_input_configs()?;
    let input_config = supported_input_configs
        .find(|config| {
            config.sample_format() == SampleFormat::F32
                && config.min_sample_rate() <= SAMPLE_RATE
                && config.max_sample_rate() >= SAMPLE_RATE
                && config.channels() <= 1 // 通常麦克风选单声道
        })
        .map(|config| config.with_sample_rate(SAMPLE_RATE))
        .ok_or(error::Error::UnsupportedInputSampleFormat)?;

    let input_config: StreamConfig = input_config.into();

    // ---------------- Output 配置 ----------------

    let output_device = host
        .default_output_device()
        .ok_or(error::Error::OutputDeviceInitError)?;

    let mut supported_output_configs = output_device.supported_output_configs()?;
    let output_config = supported_output_configs
        .find(|config| {
            config.sample_format() == SampleFormat::F32
                && config.min_sample_rate() <= SAMPLE_RATE
                && config.max_sample_rate() >= SAMPLE_RATE
                && config.channels() <= 2 // 允许单声道或立体声
        })
        .map(|config| config.with_sample_rate(SAMPLE_RATE))
        .ok_or(error::Error::UnsupportedOutputSampleFormat)?;

    let output_config: StreamConfig = output_config.into();

    // 【关键修改 1】获取实际协商好的输出声道数 (例如：1 或 2)
    let output_channels = output_config.channels as usize;

    // ---------------- 建立流 ----------------

    let input_stream = input_device.build_input_stream(
        &input_config,
        move |data: &[f32], _| {
            // 将录制的数据推入 buffer
            mic_prod.push_slice(data);
        },
        |err| panic!("input error: {:?}", err),
        None,
    )?;

    let output_stream = output_device.build_output_stream(
        &output_config,
        move |output: &mut [f32], _| {
            // 使用动态的声道数进行切片
            // 之前是 chunks_exact_mut(2)，如果是单声道设备就会导致数据写入错位（慢放）
            for frame in output.chunks_exact_mut(output_channels) {
                if let Some(sample) = speaker_cons.try_pop() {
                    // 将这一个采样复制到该帧的所有声道中
                    // 如果是单声道，frame 长度为1；如果是立体声，frame 长度为2，都会被填入 sample
                    for channel_sample in frame.iter_mut() {
                        *channel_sample = sample;
                    }
                } else {
                    // 缓冲区为空时填充静音
                    for channel_sample in frame.iter_mut() {
                        *channel_sample = 0.0;
                    }
                }
            }
        },
        |err| panic!("output error: {:?}", err),
        None,
    )?;

    // ---------------- 启动处理线程 ----------------

    // mic input audio process thread
    let _audio_process = std::thread::Builder::new()
        .name("Audio Pipeline Thread".to_owned())
        .spawn(move || {
            let mut filter =
                AudioProcessor::new(mic_cons, processed_cons, processed_prod, speaker_prod);
            loop {
                filter.process();
                // 注意：sleep 时间如果不精准可能会导致 buffer 堆积或欠载
                // 建议根据 buffer 剩余量动态 sleep，或者简单地 sleep 小一点的值
                std::thread::sleep(Duration::from_millis(5));
            }
        });

    // ---------------- 开始播放 ----------------

    input_stream.play()?;
    output_stream.play()?;

    println!("Audio system running. Channels: {}", output_channels);

    // 保持主线程运行
    loop {
        println!("Running...");
        std::thread::sleep(Duration::from_secs(5));
    }
}
