use fdaf_aec::FdafAec;
use ringbuf::{
    HeapCons, HeapProd,
    traits::{Consumer, Observer, Producer},
};

use crate::constant::*;

pub fn audio_processing(
    mut mic_cons: HeapCons<f32>,
    mut far_end_cons: HeapCons<f32>,
    mut processed_prod: HeapProd<f32>,
) {
    let mut aec = FdafAec::<AEC_FFT_SIZE>::new(STEP_SIZE);

    loop {
        while mic_cons.occupied_len() >= AEC_FRAME_SIZE {
            let mut mic_frame = [0f32; AEC_FRAME_SIZE];
            let mut speaker_frame = [0f32; AEC_FRAME_SIZE];
            let mut output_frame = [0f32; AEC_FRAME_SIZE];

            mic_cons.pop_slice(&mut mic_frame);
            // if dbg!(get_far_end.occupied_len()) >= AEC_FRAME_SIZE {
            far_end_cons.pop_slice(&mut speaker_frame);
            // }

            aec.process(
                output_frame.first_chunk_mut::<AEC_FRAME_SIZE>().unwrap(),
                speaker_frame.first_chunk().unwrap(),
                mic_frame.first_chunk().unwrap(),
            );

            //
            processed_prod.push_slice(&output_frame);
        }
    }
}
