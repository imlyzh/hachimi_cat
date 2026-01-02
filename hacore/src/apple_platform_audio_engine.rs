use std::{sync::Arc, thread::JoinHandle};

use coreaudio::audio_unit::{
    AudioUnit, IOType, Scope, StreamFormat,
    audio_format::LinearPcmFlags,
    render_callback::{Args, data::NonInterleaved},
};
use ringbuf::{HeapCons, HeapRb, traits::*};

// use libhachimi::audio_processing::AudioProcessor;
use libhachimi::{AudioProcessor, constant::*};

use crate::{
    AudioEngine, DecodeCommand, EngineBuilder, FRAME10MS, FRAME20MS,
    apple_platform_audio_processor::ApplePlatformAudioProcessor,
};

// use coreaudio::audio_unit::

pub struct ApplePlatformAudioEngine {
    vpio_input_unit: AudioUnit,
    vpio_output_unit: AudioUnit,
    pub decode_process: Arc<JoinHandle<()>>,
}

// FIXME
unsafe impl Send for ApplePlatformAudioEngine {}
unsafe impl Sync for ApplePlatformAudioEngine {}

impl EngineBuilder for ApplePlatformAudioEngine {
    /// # Safety
    /// This function is **non-reentrant**. The caller must ensure that
    /// no two threads enter this function simultaneously.
    /// TODO: Rewrite this function.
    fn build(
        encoder_output: tokio::sync::mpsc::Sender<Vec<u8>>,
        decoder_input: HeapCons<DecodeCommand>,
    ) -> anyhow::Result<Arc<Self>> {
        // config
        let mut vpio_input_unit = AudioUnit::new(IOType::VoiceProcessingIO)?;
        let mut vpio_output_unit = AudioUnit::new(IOType::VoiceProcessingIO)?;
        vpio_input_unit.set_stream_format(
            StreamFormat {
                sample_rate: 48000f64,
                sample_format: coreaudio::audio_unit::SampleFormat::F32,
                flags: LinearPcmFlags::IS_FLOAT
                    | LinearPcmFlags::IS_PACKED
                    | LinearPcmFlags::IS_NON_INTERLEAVED,
                channels: 1,
            },
            Scope::Output,
            coreaudio::audio_unit::Element::Input,
        )?;
        vpio_output_unit.set_stream_format(
            StreamFormat {
                sample_rate: 48000f64,
                sample_format: coreaudio::audio_unit::SampleFormat::F32,
                flags: LinearPcmFlags::IS_FLOAT
                    | LinearPcmFlags::IS_PACKED
                    | LinearPcmFlags::IS_NON_INTERLEAVED,
                channels: 2,
            },
            Scope::Input,
            coreaudio::audio_unit::Element::Output,
        )?;

        // buffer init

        let mic_buf = HeapRb::<f32>::new(FRAME20MS * 2);
        let (mut mic_prod, mic_cons) = mic_buf.split();

        let speaker_buf = HeapRb::<f32>::new(FRAME20MS * 2);
        let (speaker_prod, mut speaker_cons) = speaker_buf.split();

        let ap_to_encoder = HeapRb::<f32>::new(FRAME20MS * 2);
        let (ap_mic_output, mut encoder_input) = ap_to_encoder.split();

        let decoder_to_ap = HeapRb::<f32>::new(FRAME20MS * 2);
        let (mut decoder_output, ap_ref_input) = decoder_to_ap.split();

        // start threads

        let encode_process = std::thread::Builder::new()
            .name("Audio Encoder Thread".to_owned())
            .spawn(move || {
                let mut encoder =
                    opus::Encoder::new(SAMPLE_RATE, opus::Channels::Mono, opus::Application::Voip)
                        .unwrap();
                encoder.set_bitrate(opus::Bitrate::Auto).unwrap();
                encoder.set_vbr(true).unwrap();
                // encoder.set_inband_fec(true).unwrap();
                // encoder.set_packet_loss_perc(0).unwrap();

                let mut frame = [0f32; FRAME10MS];
                let mut output = [0u8; 4096];

                loop {
                    while encoder_input.occupied_len() >= FRAME10MS {
                        encoder_input.pop_slice(&mut frame);
                        let encode_size = encoder.encode_float(&frame, &mut output).unwrap();
                        let _ = encoder_output.try_send(output[..encode_size].to_vec());
                    }
                    std::thread::park();
                }
            })
            .unwrap();

        let audio_process = std::thread::Builder::new()
            .name("Audio Pipeline Thread".to_owned())
            .spawn(move || {
                let mut ap = ApplePlatformAudioProcessor::build().unwrap();
                let mut mic_cons = mic_cons;
                let mut ap_ref_input = ap_ref_input;
                let mut ap_mic_output = ap_mic_output;
                let mut speaker_prod = speaker_prod;
                loop {
                    ap.process(
                        &mut mic_cons,
                        &mut ap_ref_input,
                        &mut ap_mic_output,
                        &mut speaker_prod,
                    );
                    encode_process.thread().unpark();
                    std::thread::park();
                }
            })
            .unwrap();
        let audio_process = Arc::new(audio_process);
        let audio_process_0 = audio_process.clone();
        let audio_process_1 = audio_process.clone();
        // let audio_process_2 = audio_process.clone();

        let decode_process = std::thread::Builder::new()
            .name("Audio Encoder Thread".to_owned())
            .spawn(move || {
                let mut decoder = opus::Decoder::new(SAMPLE_RATE, opus::Channels::Mono).unwrap();
                let mut decoder_input = decoder_input;

                let mut frame = [0f32; FRAME10MS];

                loop {
                    if decoder_output.vacant_len() >= FRAME10MS {
                        let decode_size = match decoder_input.try_pop() {
                            Some(DecodeCommand::DecodeNormal(packet)) => {
                                decoder.decode_float(&packet, &mut frame, false).unwrap()
                            }
                            Some(DecodeCommand::DecodeFEC(packet)) => {
                                decoder.decode_float(&packet, &mut frame, true).unwrap()
                            }
                            Some(DecodeCommand::DecodePLC) | None => {
                                decoder.decode_float(&[], &mut frame, false).unwrap()
                            }
                        };
                        decoder_output.push_slice(&frame[..decode_size]);
                        audio_process_0.thread().unpark();
                    }
                    std::thread::park_timeout(std::time::Duration::from_millis(10));
                }
            })
            .unwrap();
        let decode_process = Arc::new(decode_process);

        vpio_input_unit.set_input_callback(move |args: Args<NonInterleaved<f32>>| {
            let Args { data, .. } = args;
            for channel in data.channels() {
                mic_prod.push_slice(channel);
                audio_process_1.thread().unpark();
            }
            Ok(())
        })?;

        vpio_output_unit.set_render_callback(move |args: Args<NonInterleaved<f32>>| {
            let Args { mut data, .. } = args;
            // 只能象征性催一下
            // audio_process_2.thread().unpark();
            for channel in data.channels_mut() {
                if let Some(sample) = speaker_cons.try_pop() {
                    for channel_sample in channel.iter_mut() {
                        *channel_sample = sample;
                    }
                } else {
                    for channel_sample in channel.iter_mut() {
                        *channel_sample = 0.0;
                    }
                }
            }
            Ok(())
        })?;

        vpio_input_unit.start()?;
        vpio_output_unit.start()?;
        println!("Audio system running.");

        Ok(Arc::new(ApplePlatformAudioEngine {
            decode_process,
            vpio_input_unit,
            vpio_output_unit,
        }))
    }
}

impl AudioEngine for ApplePlatformAudioEngine {
    // fn notify_decoder(&self) {
    //     self.decode_process.thread().unpark();
    // }
    fn get_decoder_thread(&self) -> Arc<JoinHandle<()>> {
        self.decode_process.clone()
    }

    fn play(&mut self) -> anyhow::Result<()> {
        // reset pipelie ringbuffer
        self.vpio_input_unit.start()?;
        self.vpio_output_unit.start()?;
        Ok(())
    }

    fn pause(&mut self) -> anyhow::Result<()> {
        self.vpio_input_unit.stop()?;
        self.vpio_output_unit.stop()?;
        Ok(())
    }

    fn enable_mic(&mut self) -> anyhow::Result<()> {
        self.vpio_input_unit.start()?;
        Ok(())
    }

    fn disable_mic(&mut self) -> anyhow::Result<()> {
        self.vpio_input_unit.stop()?;
        Ok(())
    }
}
