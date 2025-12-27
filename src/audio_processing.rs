use std::time::Duration;

use biquad::*;
use fdaf_aec::FdafAec;
use nnnoiseless::DenoiseState;
use ringbuf::{
    HeapCons, HeapProd, LocalRb,
    storage::Heap,
    traits::{Consumer, Observer, Producer, Split},
};

use crate::{constant::*, limiter::SmoothLimiter, noise_gate::*};

pub fn audio_processing(
    mut mic_cons: HeapCons<f32>,
    mut ref_cons: HeapCons<f32>,
    mut mic_prod: HeapProd<f32>,
    mut ref_prod: HeapProd<f32>,
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
    let mut ref_limiter = SmoothLimiter::new(0.9, 0.1, 80.0, SAMPLE_RATE as f32);
    let mut noise_gate = VoipSoftGate::new(0.01, 0.001, 1.0, 80.0, SAMPLE_RATE as f32);
    let mut aec_state = FdafAec::<AEC_FFT_SIZE>::new(STEP_SIZE, 0.9, 10e-4);
    let mut denoise = DenoiseState::new();
    let mut mic_hpfilter = DirectForm2Transposed::<f32>::new(coeffs);
    let mut far_end_hpfilter = DirectForm2Transposed::<f32>::new(coeffs);
    let mut nlp_filter = DirectForm2Transposed::<f32>::new(coeffs);

    // local ringbuffer
    let ref_limit_rb = LocalRb::<Heap<f32>>::new(FRAME_SIZE * 4);
    let (mut ref_limit_prod, mut ref_limit_cons) = ref_limit_rb.split();
    let dispatch_rb = LocalRb::<Heap<f32>>::new(FRAME_SIZE * 4);
    let (mut dispatch_prod, mut dispatch_cons) = dispatch_rb.split();

    let hpf_mic_rb = LocalRb::<Heap<f32>>::new(FRAME_SIZE.max(AEC_FRAME_SIZE) * 4);
    let (mut hpf_mic_prod, mut hpf_mic_cons) = hpf_mic_rb.split();
    let hpf_ref_rb = LocalRb::<Heap<f32>>::new(FRAME_SIZE.max(AEC_FRAME_SIZE) * 4);
    let (mut hpf_ref_prod, mut hpf_ref_cons) = hpf_ref_rb.split();

    let aec_rb = LocalRb::<Heap<f32>>::new(FRAME_SIZE.max(AEC_FRAME_SIZE) * 4);
    let (mut aec_prod, mut aec_cons) = aec_rb.split();

    let nlp_rb = LocalRb::<Heap<f32>>::new(FRAME_SIZE.max(AEC_FRAME_SIZE) * 4);
    let (mut nlp_prod, mut nlp_cons) = nlp_rb.split();

    // signal process main loop
    loop {
        // pre process mic input HighPassFilter
        hpf(&mut mic_hpfilter, &mut mic_cons, &mut hpf_mic_prod);
        // pre process far end ref
        limit(&mut ref_limiter, &mut ref_cons, &mut ref_limit_prod);

        // ref dispatch
        while ref_limit_cons.occupied_len() >= FRAME_SIZE
            && ref_prod.vacant_len() >= FRAME_SIZE
            && hpf_ref_prod.vacant_len() >= FRAME_SIZE
        {
            let mut frame = [0f32; FRAME_SIZE];
            ref_limit_cons.pop_slice(&mut frame);
            ref_prod.push_slice(&frame);
            dispatch_prod.push_slice(&frame);
        }

        // pre process far end ref HighPassFilter
        hpf(&mut far_end_hpfilter, &mut dispatch_cons, &mut hpf_ref_prod);

        // aec (echo cancel)
        aec(
            &mut aec_state,
            &mut hpf_mic_cons,
            &mut hpf_ref_cons,
            &mut aec_prod,
        );

        nlp(
            &mut nlp_filter,
            &mut noise_gate,
            &mut aec_cons,
            &mut nlp_prod,
        );

        noiseless(&mut denoise, &mut nlp_cons, &mut mic_prod);

        std::thread::sleep(Duration::from_millis(16));
    }
}

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

pub fn limit(
    limiter: &mut SmoothLimiter,
    cons: &mut impl Consumer<Item = f32>,
    prod: &mut impl Producer<Item = f32>,
) {
    let mut frame = [0f32; FRAME_SIZE];
    while cons.occupied_len() >= FRAME_SIZE && prod.vacant_len() >= FRAME_SIZE {
        cons.pop_slice(&mut frame);
        sanitize(&mut frame);
        limiter.process(&mut frame);
        prod.push_slice(&frame);
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

        sanitize(&mut output_frame);
        prod.push_slice(&output_frame);
    }
}

pub fn nlp(
    nlp_filter: &mut DirectForm2Transposed<f32>,
    noise_gate: &mut VoipSoftGate,
    cons: &mut impl Consumer<Item = f32>,
    prod: &mut impl Producer<Item = f32>,
) {
    let mut nlp_frame = [0f32; FRAME_SIZE];

    while cons.occupied_len() >= FRAME_SIZE && prod.vacant_len() >= FRAME_SIZE {
        cons.pop_slice(&mut nlp_frame);
        for i in nlp_frame.iter_mut() {
            *i = nlp_filter.run(*i);
        }
        noise_gate.process(&mut nlp_frame);
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

        sanitize(&mut ns_output_frame);
        prod.push_slice(&ns_output_frame);
    }
}

fn sanitize(frame: &mut [f32]) {
    for x in frame.iter_mut() {
        let val = if x.is_finite() { *x } else { 0f32 };
        *x = val.clamp(-1.0, 1.0);
    }
}
