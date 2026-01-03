use libhachimi::AudioProcessor;
use ringbuf::{
    HeapCons, HeapProd,
    traits::{Consumer, Observer, Producer},
};

use crate::FRAME10MS;

pub struct EmptyAudioProcessor {}

impl EmptyAudioProcessor {
    pub fn build() -> anyhow::Result<Self> {
        Ok(EmptyAudioProcessor {})
    }
}
impl AudioProcessor for EmptyAudioProcessor {
    fn process(
        &mut self,
        mic_cons: &mut HeapCons<f32>,
        ref_cons: &mut HeapCons<f32>,
        mic_prod: &mut HeapProd<f32>,
        ref_prod: &mut HeapProd<f32>,
    ) {
        let mut mic_frame = [0f32; FRAME10MS];
        let mut ref_frame = [0f32; FRAME10MS];

        while mic_cons.occupied_len() >= FRAME10MS
            && ref_cons.occupied_len() >= FRAME10MS
            && mic_prod.vacant_len() >= FRAME10MS
            && ref_prod.vacant_len() >= FRAME10MS
        {
            ref_cons.pop_slice(&mut ref_frame);
            ref_prod.push_slice(&ref_frame);
            mic_cons.pop_slice(&mut mic_frame);
            mic_prod.push_slice(&mic_frame);
        }
    }
}
