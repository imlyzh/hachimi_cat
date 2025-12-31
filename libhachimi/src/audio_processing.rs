use alloc::boxed::Box;
use biquad::*;
use fdaf_aec::FdafAec;
use nnnoiseless::DenoiseState;
use ringbuf::{
    HeapCons, HeapProd, LocalRb,
    storage::Heap,
    traits::{Consumer, Observer, Producer, Split},
};

use crate::{aec_guard::AecGuard, constant::*, limiter::SmoothLimiter, noise_gate::*};

pub type BufProd = <LocalRb<Heap<f32>> as Split>::Prod;
pub type BufCons = <LocalRb<Heap<f32>> as Split>::Cons;

pub struct AudioProcessor {
    // Singal Process State Machines
    ref_limiter: SmoothLimiter,
    noise_gate: VoipSoftGate,
    aec_init_state: FdafAec<AEC_FFT_SIZE>,
    aec_state: FdafAec<AEC_FFT_SIZE>,
    aec_guard: AecGuard,
    denoise: Box<DenoiseState<'static>>,
    mic_hpfilter: DirectForm2Transposed<f32>,
    far_end_hpfilter: DirectForm2Transposed<f32>,
    nlp_filter: DirectForm2Transposed<f32>,

    // LocalRb

    // Reference Limiter Buffer
    ref_limit_prod: BufProd,
    ref_limit_cons: BufCons,

    // Dispatch Buffer
    dispatch_prod: BufProd,
    dispatch_cons: BufCons,

    // HighPassFilter Mic Buffer
    hpf_mic_prod: BufProd,
    hpf_mic_cons: BufCons,

    // HighPassFilter Ref Buffer
    hpf_ref_prod: BufProd,
    hpf_ref_cons: BufCons,

    // AEC Buffer
    aec_prod: BufProd,
    aec_cons: BufCons,

    // NLP Buffer
    nlp_prod: BufProd,
    nlp_cons: BufCons,
}

impl Default for AudioProcessor {
    fn default() -> Self {
        Self::new()
    }
}

impl AudioProcessor {
    pub fn new() -> Self {
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
        let ref_limiter = SmoothLimiter::new(0.9, 0.1, 80.0, SAMPLE_RATE as f32);
        let noise_gate = VoipSoftGate::new(0.01, 0.001, 1.0, 80.0, SAMPLE_RATE as f32);
        let aec_state = FdafAec::<AEC_FFT_SIZE>::new(STEP_SIZE, 0.9, 10e-2, 10e-6);
        let aec_guard = AecGuard::new(5, 30);
        let denoise = DenoiseState::new();
        let mic_hpfilter = DirectForm2Transposed::<f32>::new(coeffs);
        let far_end_hpfilter = DirectForm2Transposed::<f32>::new(coeffs);
        let nlp_filter = DirectForm2Transposed::<f32>::new(coeffs);

        // local ringbuffer
        let (ref_limit_prod, ref_limit_cons) = LocalRb::<Heap<f32>>::new(FRAME_SIZE * 4).split();
        let (dispatch_prod, dispatch_cons) = LocalRb::<Heap<f32>>::new(FRAME_SIZE * 4).split();

        let (hpf_mic_prod, hpf_mic_cons) =
            LocalRb::<Heap<f32>>::new(FRAME_SIZE.max(AEC_FRAME_SIZE) * 4).split();
        let (hpf_ref_prod, hpf_ref_cons) =
            LocalRb::<Heap<f32>>::new(FRAME_SIZE.max(AEC_FRAME_SIZE) * 4).split();

        let (aec_prod, aec_cons) =
            LocalRb::<Heap<f32>>::new(FRAME_SIZE.max(AEC_FRAME_SIZE) * 4).split();

        let (nlp_prod, nlp_cons) =
            LocalRb::<Heap<f32>>::new(FRAME_SIZE.max(AEC_FRAME_SIZE) * 4).split();

        Self {
            ref_limiter,
            noise_gate,
            aec_init_state: aec_state.clone(),
            aec_state,
            aec_guard,
            denoise,
            mic_hpfilter,
            far_end_hpfilter,
            nlp_filter,
            ref_limit_prod,
            ref_limit_cons,
            dispatch_prod,
            dispatch_cons,
            hpf_mic_prod,
            hpf_mic_cons,
            hpf_ref_prod,
            hpf_ref_cons,
            aec_prod,
            aec_cons,
            nlp_prod,
            nlp_cons,
        }
    }

    pub fn process(
        &mut self,
        mic_cons: &mut HeapCons<f32>,
        ref_cons: &mut HeapCons<f32>,
        mic_prod: &mut HeapProd<f32>,
        ref_prod: &mut HeapProd<f32>,
    ) {
        // pre process mic
        hpf(&mut self.mic_hpfilter, mic_cons, &mut self.hpf_mic_prod);
        // pre process far end ref
        limit(&mut self.ref_limiter, ref_cons, &mut self.ref_limit_prod);

        // ref dispatch
        while self.ref_limit_cons.occupied_len() >= FRAME_SIZE
            && ref_prod.vacant_len() >= FRAME_SIZE
            && self.hpf_ref_prod.vacant_len() >= FRAME_SIZE
        {
            let mut frame = [0f32; FRAME_SIZE];
            self.ref_limit_cons.pop_slice(&mut frame);
            ref_prod.push_slice(&frame);
            self.dispatch_prod.push_slice(&frame);
        }

        // pre process far end ref HighPassFilter
        hpf(
            &mut self.far_end_hpfilter,
            &mut self.dispatch_cons,
            &mut self.hpf_ref_prod,
        );

        // aec (echo cancel)
        aec(
            &mut self.aec_state,
            &self.aec_init_state,
            &mut self.aec_guard,
            &mut self.hpf_mic_cons,
            &mut self.hpf_ref_cons,
            &mut self.aec_prod,
        );

        nlp(
            &mut self.nlp_filter,
            &mut self.noise_gate,
            &mut self.aec_cons,
            &mut self.nlp_prod,
        );

        noiseless(&mut self.denoise, &mut self.nlp_cons, mic_prod);
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
    inited_aec: &FdafAec<AEC_FFT_SIZE>,
    guard: &mut AecGuard,
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

        if guard.examine_and_protect(&mic_frame, &mut output_frame) {
            *aec = inited_aec.clone();
        }
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

pub fn noiseless(
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

pub fn sanitize(frame: &mut [f32]) {
    for x in frame.iter_mut() {
        let val = if x.is_finite() { *x } else { 0f32 };
        *x = val.clamp(-1.0, 1.0);
    }
}
