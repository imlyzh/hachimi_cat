use std::time::Duration;

use biquad::*;
use fdaf_aec::FdafAec;
use nnnoiseless::DenoiseState;
use ringbuf::{
    HeapCons, HeapProd, LocalRb,
    storage::Heap,
    traits::{Consumer, Producer, Split},
};

use crate::constant::*;

pub fn audio_processing(
    mut mic_cons: HeapCons<f32>,
    mut far_end_cons: HeapCons<f32>,
    mut processed_prod: HeapProd<f32>,
) {
    let coeffs = Coefficients::<f32>::from_params(
        Type::HighPass,
        FILTER_SAMPLE.hz(),
        FILTER_LOW_FRE.hz(),
        Q_BUTTERWORTH_F32,
    )
    .expect("Failed to create coefficients");

    // let nlp_lpfilter = Coefficients::<f32>::from_params(
    // Type::LowPass,
    // FILTER_SAMPLE.hz(),
    // FILTER_HIGH_FRE.hz(),
    // Q_BUTTERWORTH_F32,
    // )
    // .expect("Failed to create coefficients");

    // state machine init
    let mut aec_state = FdafAec::<AEC_FFT_SIZE>::new(STEP_SIZE, 0.9, 10e-8);
    let mut denoise = DenoiseState::new();
    let mut mic_hpfilter = DirectForm2Transposed::<f32>::new(coeffs);
    let mut far_end_hpfilter = DirectForm2Transposed::<f32>::new(coeffs);
    let mut nlp_filter = DirectForm2Transposed::<f32>::new(coeffs);
    // let mut nlp_lpfilter = DirectForm2Transposed::<f32>::new(nlp_lpfilter);

    // local ringbuffer
    let hpf_mic_to_aec = LocalRb::<Heap<f32>>::new(FRAME_SIZE.max(AEC_FRAME_SIZE) * 4);
    let (mut hpf_mic_prod, mut aec_mic_cons) = hpf_mic_to_aec.split();
    let hpf_far_end_to_aec = LocalRb::<Heap<f32>>::new(FRAME_SIZE.max(AEC_FRAME_SIZE) * 4);
    let (mut hpf_far_end_prod, mut aec_far_end_cons) = hpf_far_end_to_aec.split();

    let aec_to_nlp = LocalRb::<Heap<f32>>::new(FRAME_SIZE.max(AEC_FRAME_SIZE) * 4);
    let (mut aec_prod, mut nlp_cons) = aec_to_nlp.split();

    let nlp_to_ns = LocalRb::<Heap<f32>>::new(FRAME_SIZE.max(AEC_FRAME_SIZE) * 4);
    let (mut nlp_prod, mut ns_cons) = nlp_to_ns.split();

    // signal process main loop
    loop {
        // pre mic input HighPassFilter
        hpf(&mut mic_hpfilter, &mut mic_cons, &mut hpf_mic_prod);

        // pre far end HighPassFilter
        // FIXME: move to output thread
        hpf(
            &mut far_end_hpfilter,
            &mut far_end_cons,
            &mut hpf_far_end_prod,
        );

        // aec (echo cancel)
        aec(
            &mut aec_state,
            &mut aec_mic_cons,
            &mut aec_far_end_cons,
            &mut aec_prod,
        );

        nlp(&mut nlp_filter, &mut nlp_cons, &mut nlp_prod);

        noiseless(&mut denoise, &mut ns_cons, &mut processed_prod);

        std::thread::sleep(Duration::from_millis(16));
    }
}

#[inline(always)]
pub fn hpf(
    filter: &mut DirectForm2Transposed<f32>,
    cons: &mut impl Consumer<Item = f32>,
    prod: &mut impl Producer<Item = f32>,
) {
    let mut hpf_frame = [0f32; FRAME_SIZE];

    while cons.occupied_len() >= FRAME_SIZE && prod.vacant_len() >= FRAME_SIZE {
        cons.pop_slice(&mut hpf_frame);
        sanitize(&mut hpf_frame);
        for i in hpf_frame.iter_mut() {
            *i = filter.run(*i);
        }
        prod.push_slice(&hpf_frame);
    }
}

pub fn aec(
    aec: &mut FdafAec<AEC_FFT_SIZE>,
    mic_cons: &mut impl Consumer<Item = f32>,
    ref_cons: &mut impl Consumer<Item = f32>,
    prod: &mut impl Producer<Item = f32>,
) {
    let mut mic_frame = [0f32; AEC_FRAME_SIZE];
    let mut ref_frame = [0f32; AEC_FRAME_SIZE];
    let mut output_frame = [0f32; AEC_FRAME_SIZE];

    while mic_cons.occupied_len() >= AEC_FRAME_SIZE && prod.vacant_len() >= AEC_FRAME_SIZE {
        mic_cons.pop_slice(&mut mic_frame);
        if ref_cons.occupied_len() >= AEC_FRAME_SIZE {
            ref_cons.pop_slice(&mut ref_frame);
        } else {
            ref_frame = [0.0; AEC_FRAME_SIZE];
        }

        aec.process(
            output_frame.first_chunk_mut::<AEC_FRAME_SIZE>().unwrap(),
            ref_frame.first_chunk().unwrap(),
            mic_frame.first_chunk().unwrap(),
        );

        // processed_prod.push_slice(&aec_output_frame);
        prod.push_slice(&output_frame);
    }
}

pub fn nlp(
    nlp_filter: &mut DirectForm2Transposed<f32>,
    cons: &mut impl Consumer<Item = f32>,
    prod: &mut impl Producer<Item = f32>,
) {
    let mut nlp_frame = [0f32; FRAME_SIZE];

    while cons.occupied_len() >= FRAME_SIZE && prod.vacant_len() >= FRAME_SIZE {
        cons.pop_slice(&mut nlp_frame);
        for i in nlp_frame.iter_mut() {
            *i = nlp_filter.run(*i);
            // *i = nlp_lpfilter.run(*i);
        }
        // TODO: noise gate
        sanitize(&mut nlp_frame);
        prod.push_slice(&nlp_frame);
    }
}

fn noiseless(
    denoise: &mut DenoiseState,
    cons: &mut impl Consumer<Item = f32>,
    prod: &mut impl Producer<Item = f32>,
) {
    let mut ns_input_frame = [0.0; DenoiseState::FRAME_SIZE];
    let mut ns_output_frame = [0.0; DenoiseState::FRAME_SIZE];

    while cons.occupied_len() >= DenoiseState::FRAME_SIZE
        && prod.vacant_len() >= DenoiseState::FRAME_SIZE
    {
        cons.pop_slice(&mut ns_input_frame);

        for i in ns_input_frame.iter_mut() {
            *i *= 32767.0f32;
        }

        denoise.process_frame(&mut ns_output_frame, &ns_input_frame);

        for i in ns_output_frame.iter_mut() {
            *i /= 32767.0f32;
        }

        safety_out(&mut ns_output_frame);
        prod.push_slice(&ns_output_frame);
    }
}

fn sanitize(frame: &mut [f32]) {
    for x in frame.iter_mut() {
        let val = if x.is_finite() { *x } else { 0f32 };
        *x = val.clamp(-0.9, 0.9);
    }
}

fn safety_out(frame: &mut [f32]) {
    for s in frame.iter_mut() {
        *s = if s.tanh().abs() > 0.99 { 0.0 } else { *s };
    }
}
