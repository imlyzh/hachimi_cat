use crate::{constant::SAMPLE_RATE, limiter::SmoothLimiter};

#[derive(Debug, Clone, Copy)]
pub struct AecGuard {
    init_limiter: SmoothLimiter,
    limiter: SmoothLimiter,
    assume_frame: usize,
    trigger_threshold: usize,
    cooldown_remaining: usize,
    cooldown_limit_frame: usize,
}

impl AecGuard {
    pub fn new(trigger_threshold: usize, cooldown_limit_frame: usize) -> Self {
        let limiter = SmoothLimiter::new(0.0001, 10.0, 100.0, SAMPLE_RATE as f32);
        Self {
            init_limiter: limiter,
            limiter,
            trigger_threshold,
            assume_frame: 0,
            cooldown_remaining: 0,
            cooldown_limit_frame,
        }
    }

    /// `return`: is_diverged
    pub fn examine_and_protect<const FRAME_SIZE: usize>(
        &mut self,
        mic_frame: &[f32; FRAME_SIZE],
        output_frame: &mut [f32; FRAME_SIZE],
    ) -> bool {
        if self.assume_frame == self.trigger_threshold {
            self.assume_frame = 0;
            self.cooldown_remaining = self.cooldown_limit_frame;
            self.limiter = self.init_limiter;
            return true;
        }

        if self.cooldown_remaining > 0 {
            *output_frame = *mic_frame;
            self.limiter.process(output_frame);
            self.cooldown_remaining -= 1;
            return false;
        }

        if self.is_diverged(mic_frame, output_frame) {
            self.assume_frame += 1;
            return true;
        }
        false
    }

    fn is_diverged<const FRAME_SIZE: usize>(
        &self,
        mic: &[f32; FRAME_SIZE],
        out: &[f32; FRAME_SIZE],
    ) -> bool {
        let mut e_mic = 0.0f32;
        let mut e_out = 0.0f32;

        for (&m, &o) in mic.iter().zip(out.iter()) {
            if !o.is_finite() {
                return true;
            }
            e_mic += m * m;
            e_out += o * o;
        }

        (e_mic > 1e-6) && (e_out > (e_mic * 1.6))
    }
}
