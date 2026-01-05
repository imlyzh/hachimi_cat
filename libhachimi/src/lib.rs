#![no_std]
extern crate alloc;

pub mod aec_guard;
pub mod audio_processing;
pub mod constant;
pub mod error;
pub mod limiter;
pub mod noise_gate;
pub mod try_impl_aec;

pub trait AudioProcessor {
    fn process(
        &mut self,
        mic_cons: &mut rtrb::Consumer<f32>,
        ref_cons: &mut rtrb::Consumer<f32>,
        mic_prod: &mut rtrb::Producer<f32>,
        ref_prod: &mut rtrb::Producer<f32>,
    );
}
