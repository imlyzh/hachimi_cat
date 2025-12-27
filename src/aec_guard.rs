pub struct AecGuard {
    assume_frame: usize,
    trigger_threshold: usize,
    cooldown_remaining: usize,
    cooldown_limit_frame: usize,
}

impl AecGuard {
    pub fn new(trigger_threshold: usize, cooldown_limit_frame: usize) -> Self {
        Self {
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
            return true;
        }

        if self.cooldown_remaining > 0 {
            // for (out, mic) in output_frame.iter_mut().zip(mic_frame.iter()) {
            //     *out = *mic * 0.0001;
            // }
            output_frame.fill(0.0);
            self.cooldown_remaining -= 1;
            return true;
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
