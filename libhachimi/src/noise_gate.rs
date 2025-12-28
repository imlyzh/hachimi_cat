pub struct VoipSoftGate {
    threshold: f32,
    floor_gain: f32,
    attack_alpha: f32,
    release_alpha: f32,
    current_gain: f32,
    envelope: f32,
    env_release: f32,
}

impl VoipSoftGate {
    /// `threshold`: 0.005 ~ 0.02
    /// `floor_gain`: 0.001 (-60dB)
    /// `attack_ms`: 2.0ms
    /// `release_ms`: 80.0ms
    /// `sample_rate`: 48000.0
    pub fn new(
        threshold: f32,
        floor_gain: f32,
        attack_ms: f32,
        release_ms: f32,
        sample_rate: f32,
    ) -> Self {
        let attack_alpha = 1.0 - (-1.0 / (attack_ms * 0.001 * sample_rate)).exp();
        let release_alpha = 1.0 - (-1.0 / (release_ms * 0.001 * sample_rate)).exp();

        let env_release = 1.0 - (-1.0 / (10.0 * 0.001 * sample_rate)).exp();

        Self {
            threshold,
            floor_gain,
            attack_alpha,
            release_alpha,
            current_gain: floor_gain,
            envelope: 0.0,
            env_release,
        }
    }

    pub fn process(&mut self, samples: &mut [f32]) {
        for sample in samples.iter_mut() {
            let abs_sample = sample.abs();

            // exist voice
            if abs_sample > self.envelope {
                self.envelope = abs_sample;
            } else {
                self.envelope += self.env_release * (abs_sample - self.envelope);
            }

            let target_gain = if self.envelope > self.threshold {
                1.0f32
            } else {
                self.floor_gain
            };

            // smooth gain
            if target_gain > self.current_gain {
                // open gate: use attack
                self.current_gain += self.attack_alpha * (target_gain - self.current_gain);
            } else {
                // close gate: use release
                self.current_gain += self.release_alpha * (target_gain - self.current_gain);
            }

            *sample *= self.current_gain;
        }
    }

    pub fn set_threshold(&mut self, threshold: f32) {
        self.threshold = threshold;
    }
}

pub struct SimpleNoiseGate {
    threshold: f32,
    attack_coeff: f32,
    release_coeff: f32,
    envelope: f32,
}

impl SimpleNoiseGate {
    /// `threshold`: (0.0 - 1.0), recommand 0.01
    /// `attack_ms`: close gate smooth time(ms), recommand 1.0ms - 10.0ms
    /// `release_ms`: close gate smooth time(ms), recommand 20.0ms - 100.0ms
    /// `sample_rate`: 48000.0
    pub fn new(threshold: f32, attack_ms: f32, release_ms: f32, sample_rate: f32) -> Self {
        // 1.0 - exp(-1.0 / (time * sample_rate))
        let attack_coeff = 1.0 - (-1.0 / (attack_ms * 0.001 * sample_rate)).exp();
        let release_coeff = 1.0 - (-1.0 / (release_ms * 0.001 * sample_rate)).exp();

        Self {
            threshold,
            attack_coeff,
            release_coeff,
            envelope: 0.0,
        }
    }

    pub fn process(&mut self, samples: &mut [f32]) {
        for sample in samples.iter_mut() {
            let abs_sample = sample.abs();

            // Envelope Following
            if abs_sample > self.envelope {
                self.envelope += self.attack_coeff * (abs_sample - self.envelope);
            } else {
                self.envelope += self.release_coeff * (abs_sample - self.envelope);
            }

            *sample = if self.envelope < self.threshold {
                0.0
            } else {
                *sample
            };
        }
    }

    pub fn set_threshold(&mut self, new_threshold: f32) {
        self.threshold = new_threshold;
    }
}
