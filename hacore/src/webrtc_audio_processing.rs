use nnnoiseless::DenoiseState;
use ringbuf::{
    HeapCons, HeapProd,
    traits::{Consumer, Observer, Producer},
};
use webrtc_audio_processing::{
    Config, EchoCancellation, GainControl, InitializationConfig, Processor,
};

pub const FRAME10MS: usize = 480;
pub const FRAME20MS: usize = 960;

pub struct AudioProcessor {
    // Singal Process State Machines
    pre_processor: Processor,
    post_processor: Processor,
    denoise: Box<DenoiseState<'static>>,
}

impl AudioProcessor {
    pub fn build() -> anyhow::Result<Self> {
        let init_config = &InitializationConfig {
            num_capture_channels: 1,
            num_render_channels: 1,
            enable_experimental_agc: false,
            enable_intelligibility_enhancer: false,
        };

        let mut pre_config = Config::default();
        pre_config.echo_cancellation = Some(EchoCancellation {
            suppression_level: webrtc_audio_processing::EchoCancellationSuppressionLevel::Moderate,
            enable_extended_filter: true,
            enable_delay_agnostic: true,
            stream_delay_ms: None,
        });
        pre_config.enable_high_pass_filter = true;
        pre_config.noise_suppression = None;
        pre_config.gain_control = None;

        let mut post_config = Config::default();
        post_config.echo_cancellation = None;
        post_config.noise_suppression = None;
        post_config.gain_control = Some(GainControl {
            mode: webrtc_audio_processing::GainControlMode::AdaptiveDigital,
            target_level_dbfs: 3,
            compression_gain_db: 20,
            enable_limiter: true,
        });

        let mut pre_processor = Processor::new(init_config)?;
        pre_processor.set_config(pre_config);

        let mut post_processor = Processor::new(init_config)?;
        post_processor.set_config(post_config);

        let denoise = DenoiseState::new();

        Ok(Self {
            pre_processor,
            post_processor,
            denoise,
        })
    }

    pub fn process(
        &mut self,
        mic_cons: &mut HeapCons<f32>,
        ref_cons: &mut HeapCons<f32>,
        mic_prod: &mut HeapProd<f32>,
        ref_prod: &mut HeapProd<f32>,
    ) {
        let mut mic_frame = [0f32; FRAME10MS];
        let mut ref_frame = [0f32; FRAME10MS];
        let mut output_frame = [0f32; FRAME10MS];
        // ref dispatch
        while mic_cons.occupied_len() >= FRAME10MS
            && ref_cons.occupied_len() >= FRAME10MS
            && mic_cons.vacant_len() >= FRAME10MS
            && ref_cons.vacant_len() >= FRAME10MS
        {
            mic_cons.pop_slice(&mut mic_frame);
            ref_cons.pop_slice(&mut ref_frame);
            self.pre_processor
                .process_capture_frame(&mut ref_frame)
                .unwrap();
            self.pre_processor
                .process_render_frame(&mut mic_frame)
                .unwrap();

            self.denoise.process_frame(&mut output_frame, &mic_frame);

            self.post_processor
                .process_capture_frame(&mut ref_frame)
                .unwrap();
            self.post_processor
                .process_capture_frame(&mut output_frame)
                .unwrap();

            ref_prod.push_slice(&ref_frame);
            mic_prod.push_slice(&output_frame);
        }
    }
}
