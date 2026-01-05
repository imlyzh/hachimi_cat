use nnnoiseless::DenoiseState;

use crate::{AudioProcessor, FRAME10MS};

pub struct ApplePlatformAudioProcessor {
    // Singal Process State Machines
    // post_processor: Processor,
    denoise: Box<DenoiseState<'static>>,
}

impl ApplePlatformAudioProcessor {
    pub fn build() -> anyhow::Result<Self> {
        // let init_config = &InitializationConfig {
        //     num_capture_channels: 1,
        //     num_render_channels: 1,
        //     enable_experimental_agc: false,
        //     enable_intelligibility_enhancer: false,
        // };

        // let post_config = Config {
        //     echo_cancellation: None,
        //     gain_control: Some(GainControl {
        //         mode: webrtc_audio_processing::GainControlMode::AdaptiveDigital,
        //         target_level_dbfs: 3,
        //         compression_gain_db: 20,
        //         enable_limiter: true,
        //     }),
        //     noise_suppression: None,
        //     voice_detection: None,
        //     enable_transient_suppressor: false,
        //     enable_high_pass_filter: false,
        // };

        // let mut post_processor = Processor::new(init_config)?;
        // post_processor.set_config(post_config);

        let denoise = DenoiseState::new();

        Ok(Self {
            // post_processor,
            denoise,
        })
    }
}

impl AudioProcessor for ApplePlatformAudioProcessor {
    fn process(
        &mut self,
        mic_cons: &mut rtrb::Consumer<f32>,
        ref_cons: &mut rtrb::Consumer<f32>,
        mic_prod: &mut rtrb::Producer<f32>,
        ref_prod: &mut rtrb::Producer<f32>,
    ) {
        while let (Ok(mic_cons), Ok(ref_cons), Ok(mut mic_prod), Ok(mut ref_prod)) = (
            mic_cons.read_chunk(FRAME10MS),
            ref_cons.read_chunk(FRAME10MS),
            mic_prod.write_chunk(FRAME10MS),
            ref_prod.write_chunk(FRAME10MS),
        ) {
            ref_prod
                .as_mut_slices()
                .0
                .copy_from_slice(ref_cons.as_slices().0);
            ref_cons.commit_all();
            ref_prod.commit_all();
            self.denoise
                .process_frame(mic_prod.as_mut_slices().0, mic_cons.as_slices().0);
            // self.post_processor
            //     .process_capture_frame(&mut output_frame)
            //     .unwrap();
            mic_cons.commit_all();
            mic_prod.commit_all();
        }
    }
}
