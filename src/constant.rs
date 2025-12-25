use cpal::SampleRate;

pub const SAMPLE_RATE: usize = 48000;
pub const FRAME_SIZE: usize = SAMPLE_RATE / 100; // 10ms
pub const RB_SIZE: usize = FRAME_SIZE * 8;
pub const TARGET_RATE: SampleRate = SampleRate(SAMPLE_RATE as u32);

pub const AEC_FRAME_SIZE: usize = 1024;
pub const AEC_FFT_SIZE: usize = AEC_FRAME_SIZE * 2;
pub const STEP_SIZE: f32 = 0.01;
