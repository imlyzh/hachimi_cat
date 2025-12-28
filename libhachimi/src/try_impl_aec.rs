use alloc::sync::Arc;
use num_complex::Complex32;
use rustfft::{Fft, FftPlanner};

/// Production-grade PBFDAF AEC
/// L: Frame size (512)
/// N: FFT size (2*L = 1024)
/// K: Number of blocks (4-8)
/// MAX_D: Max delay blocks
pub struct PbfdafAec<const L: usize, const N: usize, const K: usize, const MAX_D: usize> {
    w: [[Complex32; N]; K],      // Filter weights (Freq domain)
    x_hist: [[Complex32; N]; K], // Reference history (Freq domain)
    hist_ptr: usize,             // Circular pointer for x_hist
    power_est: [f32; N],         // Power Spectral Density estimate
    last_ref: [f32; L],          // Previous ref frame for Overlap-Save

    delay_ring: [[f32; L]; MAX_D], // Fixed-size delay buffer
    delay_rd: usize,
    delay_wr: usize,
    target_delay: usize,

    fft_fwd: Arc<dyn Fft<f32>>,
    fft_inv: Arc<dyn Fft<f32>>,

    mu: f32,    // Step size
    alpha: f32, // Power smoothing factor
    eps: f32,   // Regularization (Epsilon)
    leaky: f32, // Weight leakage factor
}

impl<const L: usize, const N: usize, const K: usize, const MAX_D: usize> PbfdafAec<L, N, K, MAX_D> {
    pub fn new(mu: f32, initial_delay: usize) -> Self {
        assert_eq!(N, 2 * L, "N must be 2 * L");
        let mut planner = FftPlanner::new();
        let fft_fwd = planner.plan_fft_forward(N);
        let fft_inv = planner.plan_fft_inverse(N);

        Self {
            w: [[Complex32::default(); N]; K],
            x_hist: [[Complex32::default(); N]; K],
            hist_ptr: 0,
            power_est: [0.5; N],
            last_ref: [0.0; L],
            delay_ring: [[0.0; L]; MAX_D],
            delay_rd: 0,
            delay_wr: initial_delay % MAX_D,
            target_delay: initial_delay,
            fft_fwd,
            fft_inv,
            mu,
            alpha: 0.95,
            eps: 0.01,
            leaky: 0.9999,
        }
    }

    pub fn set_delay(&mut self, blocks: usize) {
        self.target_delay = blocks % MAX_D;
    }

    pub fn process(&mut self, error_out: &mut [f32; L], raw_ref: &[f32; L], mic: &[f32; L]) {
        let inv_n = 1.0 / N as f32;

        // 1. Delay Buffer Management
        self.delay_ring[self.delay_wr] = *raw_ref;
        self.delay_wr = (self.delay_wr + 1) % MAX_D;
        let ref_frame = &self.delay_ring[self.delay_rd];
        self.delay_rd = (self.delay_rd + 1) % MAX_D;

        // 2. Reference FFT (Overlap-Save)
        let mut x_time = [Complex32::default(); N];
        for i in 0..L {
            x_time[i] = Complex32::new(self.last_ref[i], 0.0);
            x_time[i + L] = Complex32::new(ref_frame[i], 0.0);
        }
        self.last_ref.copy_from_slice(ref_frame);
        self.fft_fwd.process(&mut x_time);

        // Update Reference History
        self.hist_ptr = if self.hist_ptr == 0 {
            K - 1
        } else {
            self.hist_ptr - 1
        };
        self.x_hist[self.hist_ptr].copy_from_slice(&x_time);

        // 3. Echo Prediction (Filtering)
        let mut y_fft = [Complex32::default(); N];
        for k in 0..K {
            let x_idx = (self.hist_ptr + k) % K;
            for (f, item) in y_fft.iter_mut().enumerate().take(N) {
                *item += self.w[k][f] * self.x_hist[x_idx][f];
            }
        }

        let mut y_time = y_fft;
        self.fft_inv.process(&mut y_time);

        // 4. Error Calculation (Time Domain)
        let mut mic_eng = 1e-6f32;
        let mut err_eng = 1e-6f32;
        for i in 0..L {
            let echo_est = y_time[i + L].re * inv_n;
            error_out[i] = mic[i] - echo_est;
            mic_eng += mic[i] * mic[i];
            err_eng += error_out[i] * error_out[i];
        }

        // 5. Weight Update (Frequency Domain)
        let mut e_fft = [Complex32::default(); N];
        for i in 0..L {
            e_fft[i + L] = Complex32::new(error_out[i], 0.0); // Constraint: Zero-padding first L
        }
        self.fft_fwd.process(&mut e_fft);

        // Simple Double-Talk Protection
        if err_eng < mic_eng * 2.0 {
            // Update Power Estimate
            let current_x = &self.x_hist[self.hist_ptr];
            for (f, item) in current_x.iter().enumerate().take(N) {
                let p = item.norm_sqr();
                self.power_est[f] = self.alpha * self.power_est[f] + (1.0 - self.alpha) * p;
            }

            for k in 0..K {
                let x_idx = (self.hist_ptr + k) % K;
                let xk = &self.x_hist[x_idx];
                let wk = &mut self.w[k];

                for f in 0..N {
                    let step = self.mu / (self.power_est[f] + self.eps);
                    let grad = e_fft[f] * xk[f].conj();
                    wk[f] = (wk[f] * self.leaky) + (grad * step);
                }

                // 6. Weight Projection (Constraint)
                // Force linear convolution by zeroing the last L samples in time domain
                let mut w_time = *wk;
                self.fft_inv.process(&mut w_time);

                let mut w_constrained = [Complex32::default(); N];
                for i in 0..L {
                    w_constrained[i] = Complex32::new(w_time[i].re * inv_n, 0.0);
                }
                self.fft_fwd.process(&mut w_constrained);
                wk.copy_from_slice(&w_constrained);
            }
        }
    }
}
