#[derive(Debug, Clone, Copy)]
pub struct SmoothLimiter {
    threshold: f32,
    attack_coeff: f32,
    release_coeff: f32,
    current_gain: f32,
}

impl SmoothLimiter {
    /// `threshold`: 0.9 (-1dB)
    /// `attack_ms`:  0.1ms ~ 1.0ms
    /// `release_ms`: 50.0ms ~ 100.0ms
    /// `sample_rate`: 48000.0ms
    pub fn new(threshold: f32, attack_ms: f32, release_ms: f32, sample_rate: f32) -> Self {
        Self {
            threshold,
            attack_coeff: 1.0 - (-1.0 / (attack_ms * 0.001 * sample_rate)).exp(),
            release_coeff: 1.0 - (-1.0 / (release_ms * 0.001 * sample_rate)).exp(),
            current_gain: 1.0,
        }
    }

    pub fn process(&mut self, samples: &mut [f32]) {
        for sample in samples.iter_mut() {
            let abs_sample = sample.abs();

            let target_gain = if abs_sample > self.threshold {
                self.threshold / abs_sample
            } else {
                1.0
            };

            // smooth gate
            if target_gain < self.current_gain {
                self.current_gain += self.attack_coeff * (target_gain - self.current_gain);
            } else {
                self.current_gain += self.release_coeff * (target_gain - self.current_gain);
            }

            *sample = (*sample * self.current_gain).clamp(-self.threshold, self.threshold);
        }
    }
}
