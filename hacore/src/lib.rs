use std::sync::Arc;

// use bytes::Bytes;

#[cfg(target_vendor = "apple")]
pub mod apple_platform_audio_engine;
#[cfg(target_vendor = "apple")]
pub mod apple_platform_audio_processor;
pub mod cross_platform_audio_processor;
pub mod default_audio_engine;
pub mod empty_audio_processor;
pub mod error;

// use libhachimi::audio_processing::AudioProcessor;

pub const SAMPLE_RATE: u32 = 48000;
pub const FRAME10MS: usize = 480;
pub const FRAME20MS: usize = 960;

pub trait EngineBuilder {
    fn build(
        encoder_input: rtrb::Producer<f32>,
        decoder_output: rtrb::Consumer<f32>,
        encode_thread: std::thread::JoinHandle<()>,
        mixer_thread: Arc<std::thread::JoinHandle<()>>,
    ) -> anyhow::Result<Arc<Self>>;
}

pub trait AudioEngine {
    fn play(&mut self) -> anyhow::Result<()>;
    fn pause(&mut self) -> anyhow::Result<()>;
}

pub trait AudioProcessor {
    fn process(
        &mut self,
        mic_cons: &mut rtrb::Consumer<f32>,
        ref_cons: &mut rtrb::Consumer<f32>,
        mic_prod: &mut rtrb::Producer<f32>,
        ref_prod: &mut rtrb::Producer<f32>,
    );
}
