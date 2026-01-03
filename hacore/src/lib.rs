use std::{sync::Arc, thread::JoinHandle};

#[cfg(target_vendor = "apple")]
pub mod apple_platform_audio_engine;
#[cfg(target_vendor = "apple")]
pub mod apple_platform_audio_processor;
pub mod cross_platform_audio_processor;
pub mod default_audio_engine;
pub mod empty_audio_processor;

// use libhachimi::audio_processing::AudioProcessor;

pub const FRAME10MS: usize = 480;
pub const FRAME20MS: usize = 960;

#[derive(Debug, Clone)]
pub enum DecodeCommand {
    DecodeNormal(Vec<u8>),
    DecodeFEC(Vec<u8>),
    DecodePLC,
}

pub trait EngineBuilder {
    fn build(
        encoder_output: tokio::sync::mpsc::Sender<Vec<u8>>,
        decoder_input: ringbuf::HeapCons<DecodeCommand>,
    ) -> anyhow::Result<Arc<Self>>;
}

pub trait AudioEngine {
    fn get_decoder_thread(&self) -> Arc<JoinHandle<()>>;
    // fn notify_decoder(&self);

    fn play(&mut self) -> anyhow::Result<()>;

    fn pause(&mut self) -> anyhow::Result<()>;

    fn enable_mic(&mut self) -> anyhow::Result<()>;

    fn disable_mic(&mut self) -> anyhow::Result<()>;
}
