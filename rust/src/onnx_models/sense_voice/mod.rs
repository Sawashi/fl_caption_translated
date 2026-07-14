use crate::{candle_models::whisper::{model::{DecodingResult, Segment, WhisperStatus}, LaunchCaptionParams}, onnx_models::sense_voice::model::SenseVoiceModel};
mod model;
mod def;

pub async fn launch_caption<F>(
    params: LaunchCaptionParams,
    mut result_callback: F,
) -> anyhow::Result<()>
where
    F: FnMut(Vec<Segment>) + Send + 'static,
{
    use crate::audio_capture::{AudioCapture, AudioCaptureConfig, PlatformAudioCapture};
    use crate::onnx_models::vad;
    use std::time::Duration;
    use tokio::time::Instant;

    let LaunchCaptionParams {
        models,
        audio_device,
        audio_device_is_input,
        audio_language,
        cancel_token,
        tokenizer_data,
        inference_timeout,
        inference_interval_ms,
        vad_model_path,
        vad_filters_value,
        whisper_max_audio_duration,
        ..
    } = params;

    let model_path = super::find_model_path(&models, None).unwrap();
    let session = super::init_model(model_path, params.try_with_cuda)?;
    // Initialize SenseVoice model
    result_callback(_make_status_response(WhisperStatus::Loading));
    let mut model = SenseVoiceModel::from_session(session)?;

    // Load token mapping
    let tokenizer_str = std::str::from_utf8(&tokenizer_data)?;
    let tokens = model::load_tokens_from_data(tokenizer_str)?;

    // Set up audio capture config
    let audio_capture_config = AudioCaptureConfig {
        device: audio_device,
        is_input: audio_device_is_input.unwrap_or(true),
        target_sample_rate: 16000,
        target_channels: 1,
    };

    let audio_capture = PlatformAudioCapture::new(audio_capture_config)?;
    let audio_info = audio_capture.get_info();
    println!("SenseVoice Audio capture info: {:?}", audio_info);

    // Start audio capture
    let rx = audio_capture.start_capture(cancel_token.child_token())?;

    result_callback(_make_status_response(WhisperStatus::Ready));
    println!("SenseVoice Ready...");

    // Initialize audio processing state
    let mut buffered_pcm = vec![];
    let mut history_pcm = Vec::new();
    let mut last_inference_time = Instant::now();
    let mut first_inference_done = false;
    let inference_interval = Duration::from_millis(inference_interval_ms.unwrap_or(2000)); // Default 2000ms
    let max_audio_duration: usize = whisper_max_audio_duration.unwrap_or(12) as usize; // Default 12 seconds
    let language = audio_language.as_deref().unwrap_or("auto"); // SenseVoice language setting

    println!("Check and loading VAD model...");
    let mut vad_model = if let Some(vad_model_path) = vad_model_path {
        let model = vad::new_vad_model(vad_model_path, false);
        if let Ok(model) = model {
            Some(model)
        } else {
            println!("Failed to load VAD model: {:?}", model.err().unwrap());
            None
        }
    } else {
        None
    };

    // Audio processing main loop
    println!("Starting SenseVoice audio processing loop...");
    let mut debug_counter = 0;

    while !cancel_token.is_cancelled() {
        debug_counter += 1;
        if debug_counter % 500 == 0 {
            println!(
                "SenseVoice audio processing loop iteration {}, buffered_pcm.len(): {}",
                debug_counter,
                buffered_pcm.len()
            );
        }

        // Receive audio data
        let pcm = rx.recv_timeout(Duration::from_millis(100));

        if pcm.is_err() {
            let err = pcm.unwrap_err();
            if debug_counter % 1000 == 0 {
                println!(
                    "Audio recv timeout or error: {:?}, cancel_token cancelled: {}",
                    err,
                    cancel_token.is_cancelled()
                );
            }
            if cancel_token.is_cancelled() {
                break;
            }
            continue;
        }

        let pcm = pcm.unwrap();

        static mut AUDIO_RECEIVED: bool = false;
        unsafe {
            if !AUDIO_RECEIVED {
                println!(
                    "SenseVoice first audio data received: {} samples",
                    pcm.len()
                );
                AUDIO_RECEIVED = true;
            } else if pcm.len() > 0 && debug_counter % 100 == 0 {
                println!(
                    "SenseVoice audio data: {} samples (debug every 100 iterations)",
                    pcm.len()
                );
            }
        }

        buffered_pcm.extend_from_slice(&pcm);

        if buffered_pcm.len() > 0 && (buffered_pcm.len() % 16000 == 0 || debug_counter % 200 == 0) {
            println!(
                "Total buffered_pcm length: {} samples ({:.1}s)",
                buffered_pcm.len(),
                buffered_pcm.len() as f32 / 16000.0
            );
        }

        // On first start, wait for 3 seconds of data
        if !first_inference_done {
            if buffered_pcm.len() < 3 * 16000 {
                continue;
            }
            first_inference_done = true;
        }

        // Check inference interval
        let now = Instant::now();
        if now.duration_since(last_inference_time) < inference_interval {
            continue;
        }

        // Record inference start time
        let inference_start = Instant::now();

        // VAD detection
        if let Some(vad_model) = vad_model.as_mut() {
            let resampled_pcm = buffered_pcm.clone();
            let vad_result = vad_model.check_vad(resampled_pcm, vad_filters_value);
            if vad_result.is_err() {
                println!("VAD error: {:?}", vad_result.err().unwrap());
            } else {
                let vad_result = vad_result?;
                println!(
                    "SenseVoice VAD prediction: {:?} filtered_count: {:?}",
                    vad_result.prediction, vad_result.filtered_count
                );
                if vad_result.prediction > vad_filters_value.unwrap_or(0.1) {
                    buffered_pcm = vad_result.pcm_results;
                } else {
                    buffered_pcm.clear();
                    last_inference_time = Instant::now();
                    continue;
                }
            }
        }

        // Audio length management - same logic as Whisper
        let max_samples = max_audio_duration * 16000;
        let total_len = history_pcm.len() + buffered_pcm.len();

        let mut adjusted_history_pcm = history_pcm.clone();
        if total_len > max_samples {
            let excess = total_len - max_samples;
            println!(
                "SenseVoice history_pcm len: {} buffered_pcm len: {} excess: {}",
                history_pcm.len(),
                buffered_pcm.len(),
                excess
            );
            if history_pcm.len() > excess {
                adjusted_history_pcm = history_pcm[excess..].to_vec();
            } else {
                adjusted_history_pcm = Vec::new();
            }
        }

        // Merge audio data
        let mut combined_pcm = Vec::with_capacity(adjusted_history_pcm.len() + buffered_pcm.len());
        combined_pcm.extend_from_slice(&adjusted_history_pcm);
        combined_pcm.extend_from_slice(&buffered_pcm);

        history_pcm = combined_pcm.clone();
        buffered_pcm.clear();

        let pcm = combined_pcm;

        // SenseVoice feature extraction and inference
        match model::run_sensevoice_inference(
            &mut model,
            &pcm,
            language,
            &tokens,
            inference_timeout.or(Some(inference_interval)),
        ) {
            Ok(segments) => {
                let inference_duration = inference_start.elapsed();
                let audio_duration = (pcm.len() as f32 / 16000.0 * 1000.0) as u128;

                let mut result_segments = segments;
                for segment in &mut result_segments {
                    segment.reasoning_duration = Some(inference_duration.as_millis());
                    segment.reasoning_lang = Some(language.to_string());
                    segment.audio_duration = Some(audio_duration);
                }

                result_callback(result_segments);
            }
            Err(e) => {
                println!("SenseVoice inference error: {:?}", e);
                // Send error status
                result_callback(_make_status_response(WhisperStatus::Error));
            }
        }

        last_inference_time = now;
    }

    println!("SenseVoice transcription cancelled");
    result_callback(_make_status_response(WhisperStatus::Exit));
    println!("SenseVoice Exit");
    Ok(())
}


fn _make_status_response(status: WhisperStatus) -> Vec<Segment> {
    vec![Segment {
        start: 0.0,
        duration: 0.0,
        dr: DecodingResult {
            tokens: vec![],
            text: "".to_string(),
            avg_logprob: 0.0,
            no_speech_prob: 0.0,
            temperature: 0.0,
            compression_ratio: 0.0,
        },
        reasoning_duration: None,
        reasoning_lang: None,
        audio_duration: None,
        status,
    }]
}
