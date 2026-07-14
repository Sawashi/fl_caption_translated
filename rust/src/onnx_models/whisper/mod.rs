pub mod model;
pub mod multilingual;

use crate::candle_models::whisper::{
    model::{DecodingResult, Segment, WhisperStatus},
    LaunchCaptionParams,
};
use crate::onnx_models::whisper::model::WhisperModel;

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
        inference_interval_ms,
        vad_model_path,
        vad_filters_value,
        whisper_max_audio_duration,
        ..
    } = params;

    let model_path = super::find_model_path(&models, None).unwrap();
    let session = super::init_model(model_path, params.try_with_cuda)?;

    // Initialize Whisper model
    result_callback(_make_status_response(WhisperStatus::Loading));
    let mut model = WhisperModel::from_session(session)?;

    // Set up audio capture config
    let audio_capture_config = AudioCaptureConfig {
        device: audio_device,
        is_input: audio_device_is_input.unwrap_or(true),
        target_sample_rate: 16000,
        target_channels: 1,
    };

    let audio_capture = PlatformAudioCapture::new(audio_capture_config)?;
    let audio_info = audio_capture.get_info();
    println!("Whisper Audio capture info: {:?}", audio_info);

    // Start audio capture
    let rx = audio_capture.start_capture(cancel_token.child_token())?;

    result_callback(_make_status_response(WhisperStatus::Ready));
    println!("Whisper Ready...");

    // Initialize audio processing state
    let mut buffered_pcm = vec![];
    let mut history_pcm = Vec::new();
    let mut last_inference_time = Instant::now();
    let mut first_inference_done = false;
    let inference_interval = Duration::from_millis(inference_interval_ms.unwrap_or(2000)); // Default 2000ms
    let max_audio_duration: usize = whisper_max_audio_duration.unwrap_or(12) as usize; // Default 12 seconds
    let language = audio_language.as_deref(); // Whisper language setting

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
    println!("Starting Whisper audio processing loop...");
    let mut debug_counter = 0;

    while !cancel_token.is_cancelled() {
        debug_counter += 1;
        if debug_counter % 500 == 0 {
            println!(
                "Whisper audio processing loop iteration {}, buffered_pcm.len(): {}",
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
                println!("Whisper first audio data received: {} samples", pcm.len());
                AUDIO_RECEIVED = true;
            } else if pcm.len() > 0 && debug_counter % 100 == 0 {
                println!(
                    "Whisper audio data: {} samples (debug every 100 iterations)",
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
                    "Whisper VAD prediction: {:?} filtered_count: {:?}",
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

        // Audio length management
        let max_samples = max_audio_duration * 16000;
        let total_len = history_pcm.len() + buffered_pcm.len();

        let mut adjusted_history_pcm = history_pcm.clone();
        if total_len > max_samples {
            let excess = total_len - max_samples;
            println!(
                "Whisper history_pcm len: {} buffered_pcm len: {} excess: {}",
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

        // Whisper inference
        match model.inference(&pcm, language) {
            Ok(text) => {
                let inference_duration = inference_start.elapsed();

                // Create result segment
                let segment = model::create_whisper_segment(
                    text,
                    pcm.len() as f64 / 16000.0, // Audio duration in seconds
                    inference_duration.as_millis(),
                    language.map(|s| s.to_string()),
                );

                result_callback(vec![segment]);
            }
            Err(e) => {
                println!("Whisper inference error: {:?}", e);
                // Send error status
                result_callback(_make_status_response(WhisperStatus::Error));
            }
        }

        last_inference_time = now;
    }

    println!("Whisper transcription cancelled");
    result_callback(_make_status_response(WhisperStatus::Exit));
    println!("Whisper Exit");
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
