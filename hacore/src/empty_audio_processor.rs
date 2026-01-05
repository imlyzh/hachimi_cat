use crate::{AudioProcessor, FRAME10MS};

pub struct EmptyAudioProcessor {}

impl EmptyAudioProcessor {
    pub fn build() -> anyhow::Result<Self> {
        Ok(EmptyAudioProcessor {})
    }
}
impl AudioProcessor for EmptyAudioProcessor {
    fn process(
        &mut self,
        mic_cons: &mut rtrb::Consumer<f32>,
        ref_cons: &mut rtrb::Consumer<f32>,
        mic_prod: &mut rtrb::Producer<f32>,
        ref_prod: &mut rtrb::Producer<f32>,
    ) {
        while mic_cons.slots() >= FRAME10MS
            && ref_cons.slots() >= FRAME10MS
            && mic_prod.slots() >= FRAME10MS
            && ref_prod.slots() >= FRAME10MS
        {
            let r = ref_cons.read_chunk(FRAME10MS).unwrap();
            let mut w = ref_prod.write_chunk(FRAME10MS).unwrap();
            w.as_mut_slices().0.copy_from_slice(r.as_slices().0);
            r.commit_all();
            w.commit_all();
            let r = mic_cons.read_chunk(FRAME10MS).unwrap();
            let mut w = mic_prod.write_chunk(FRAME10MS).unwrap();
            w.as_mut_slices().0.copy_from_slice(r.as_slices().0);
            r.commit_all();
            w.commit_all();
        }
    }
}
