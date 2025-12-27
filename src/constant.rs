pub const SAMPLE_RATE: u32 = 48000;
pub const FRAME_SIZE: usize = SAMPLE_RATE as usize / 50; // 20ms
pub const RB_SIZE: usize = FRAME_SIZE * 4;

pub const AEC_FRAME_SIZE: usize = 512;
pub const AEC_FFT_SIZE: usize = AEC_FRAME_SIZE * 2;
pub const STEP_SIZE: f32 = 0.0001;

pub const FILTER_SAMPLE: f32 = SAMPLE_RATE as f32;
pub const FILTER_LOW_FRE: f32 = 80f32;
pub const FILTER_HIGH_FRE: f32 = 24000f32;
