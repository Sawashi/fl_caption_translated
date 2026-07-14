pub mod model;
pub mod multilingual;

use std::collections::HashMap;
use std::time::Duration;

use crate::audio_capture::{AudioCapture, AudioCaptureConfig, PlatformAudioCapture};
use crate::candle_models::whisper::model::{Model, Segment};
use crate::{get_device, onnx_models};
use candle_nn::VarBuilder;
use candle_transformers::models::whisper::{self as m, audio, Config};
use tokenizers::Tokenizer;
use tokio::time::Instant;
use tokio_util::sync::CancellationToken;

pub struct LaunchCaptionParams {
    pub models: HashMap<String, String>,
    pub config_data: String,
    pub model_type: String,
    pub is_quantized: bool,
    pub tokenizer_data: Vec<u8>,
    pub audio_device: Option<String>,
    pub audio_device_is_input: Option<bool>,
    pub audio_language: Option<String>,
    pub is_multilingual: Option<bool>,
    pub cancel_token: CancellationToken,
    pub with_timestamps: Option<bool>,
    pub verbose: Option<bool>,
    pub try_with_cuda: bool,
    pub inference_timeout: Option<Duration>, // Total inference timeout
    pub max_tokens_per_segment: Option<usize>, // Max tokens per segment to prevent hallucinations
    pub whisper_max_audio_duration: Option<u32>, // Audio context length in seconds
    pub inference_interval_ms: Option<u64>,  // Inference interval in ms
    pub whisper_temperature: Option<f32>,    // Temperature parameter
    pub vad_model_path: Option<String>,      // VAD model path
    pub vad_filters_value: Option<f32>,      // VAD filter threshold
}

pub async fn launch_caption<F>(
    params: LaunchCaptionParams,
    mut result_callback: F,
) -> anyhow::Result<()>
where
    F: FnMut(Vec<Segment>) + Send + 'static,
{
    let LaunchCaptionParams {
        models,
        config_data,
        is_quantized,
        tokenizer_data,
        audio_device,
        audio_device_is_input,
        audio_language,
        is_multilingual,
        cancel_token,
        with_timestamps,
        verbose,
        try_with_cuda,
        inference_timeout,
        max_tokens_per_segment,
        whisper_max_audio_duration,
        inference_interval_ms,
        whisper_temperature,
        vad_model_path,
        vad_filters_value,
        ..
    } = params;

    let model_path: String = models.values().next().unwrap().to_string();
    result_callback(_make_status_response(model::WhisperStatus::Loading));
    let device = get_device(try_with_cuda)?;
    let arg_is_multilingual = is_multilingual.unwrap_or(false);
    let arg_language = audio_language;
    let arg_device = audio_device;
    let is_input = audio_device_is_input.unwrap_or(true);

    let config: Config = serde_json::from_str(&config_data)?;
    let tokenizer = Tokenizer::from_bytes(tokenizer_data).unwrap();

    // check model path
    if !std::path::Path::new(&model_path).exists() {
        anyhow::bail!("model path does not exist: {model_path}");
    }

    let model = if is_quantized {
        let vb = candle_transformers::quantized_var_builder::VarBuilder::from_gguf(
            &model_path,
            &device,
        )?;
        Model::Quantized(m::quantized_model::Whisper::load(&vb, config.clone())?)
    } else {
        let vb = unsafe { VarBuilder::from_mmaped_safetensors(&[model_path], m::DTYPE, &device)? };
        Model::Normal(m::model::Whisper::load(&vb, config.clone())?)
    };
    let seed = 299792458;
    let mut decoder = model::Decoder::new(
        model,
        tokenizer.clone(),
        seed,
        &device,
        /* language_token */ None,
        Some(model::Task::Transcribe),
        with_timestamps.unwrap_or(false),
        verbose.unwrap_or(false),
    )?;

    let mel_bytes = get_mel_bytes(config.num_mel_bins)?;
    let mut mel_filters = vec![0f32; mel_bytes.len() / 4];
    <byteorder::LittleEndian as byteorder::ByteOrder>::read_f32_into(&mel_bytes, &mut mel_filters);

    // Set up audio capture using the abstracted interface
    let audio_capture_config = AudioCaptureConfig {
        device: arg_device,
        is_input,
        target_sample_rate: 16000,
        target_channels: 1,
    };

    let audio_capture = PlatformAudioCapture::new(audio_capture_config)?;
    let audio_info = audio_capture.get_info();
    println!("Audio capture info: {:?}", audio_info);

    // Start audio capture
    let rx = audio_capture.start_capture(cancel_token.child_token())?;

    result_callback(_make_status_response(model::WhisperStatus::Ready));
    println!("Whisper Ready...");

    // Processing runs in the current function
    let mut buffered_pcm = vec![];
    let mut history_pcm = Vec::new();
    let mut last_inference_time = Instant::now();
    // let mut last_vad_passed_time = Instant::now();
    let mut first_inference_done = false;
    let inference_interval = Duration::from_millis(inference_interval_ms.unwrap_or(2000)); // Default 2000ms
    let max_audio_duration: usize = whisper_max_audio_duration.unwrap_or(12) as usize; // Default 12 seconds
    let mut language_token_set = false;
    let mut language_token_name: Option<String> = None;
    let fixed_temperature = whisper_temperature;

    println!("Check and loading vad model...");
    let mut vad_model = if let Some(vad_model_path) = vad_model_path {
        // try_with_gpu: [false] candle-onnx not support gpu
        let model = onnx_models::vad::new_vad_model(vad_model_path, false);
        if let Ok(model) = model {
            Some(model)
        } else {
            println!("Failed to load VAD model: {:?}", model.err().unwrap());
            None
        }
    } else {
        None
    };

    // Processing loop
    println!("Starting audio processing loop...");
    let mut debug_counter = 0;
    while !cancel_token.is_cancelled() {
        debug_counter += 1;
        if debug_counter % 500 == 0 {
            // Print debug info every 50 seconds
            println!(
                "Audio processing loop iteration {}, buffered_pcm.len(): {}",
                debug_counter,
                buffered_pcm.len()
            );
        }

        // Try to receive audio data with a timeout to periodically check cancellation
        let pcm = rx.recv_timeout(Duration::from_millis(100));

        // If recv times out or channel is closed, check if cancellation is requested
        if pcm.is_err() {
            let err = pcm.unwrap_err();
            if debug_counter % 1000 == 0 {
                // Print timeout info every 100 seconds
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

        // Process received audio data
        let pcm = pcm.unwrap();

        static mut AUDIO_RECEIVED: bool = false;
        unsafe {
            if !AUDIO_RECEIVED {
                println!("First audio data received: {} samples", pcm.len());
                AUDIO_RECEIVED = true;
            } else if pcm.len() > 0 && debug_counter % 100 == 0 {
                println!(
                    "Audio data: {} samples (debug every 100 iterations)",
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

        // Check if enough time has passed since the last inference
        let now = Instant::now();
        if now.duration_since(last_inference_time) < inference_interval {
            continue;
        }

        // If the gap since last valid inference exceeds 2x the interval, clear history audio (accounts for natural pauses)
        // if now.duration_since(last_vad_passed_time) > inference_interval * 2 {
        //     println!("Clearing buffered_pcm due to long silence");
        //     history_pcm.clear();
        // }

        // Record inference start time
        let inference_start = Instant::now();

        if let Some(vad_model) = vad_model.as_mut() {
            let resampled_pcm = buffered_pcm.clone();
            let vad_result: Result<onnx_models::vad::VadResult, anyhow::Error> =
                vad_model.check_vad(resampled_pcm, vad_filters_value);
            if vad_result.is_err() {
                println!("VAD error: {:?}", vad_result.err().unwrap());
            } else {
                let vad_result = vad_result?;
                println!(
                    "VAD prediction: {:?} filtered_count: {:?}",
                    vad_result.prediction, vad_result.filtered_count
                );
                if vad_result.prediction > vad_filters_value.unwrap_or(0.1) {
                    buffered_pcm = vad_result.pcm_results;
                    // last_vad_passed_time = Instant::now();
                } else {
                    buffered_pcm.clear();
                    last_inference_time = Instant::now();
                    continue;
                }
            }
        }

        // Calculate max samples (using 16000 sample rate)
        let max_samples = max_audio_duration * 16000;

        // Calculate total length
        let total_len = history_pcm.len() + buffered_pcm.len();

        // If total length exceeds max sample limit, adjust history_pcm
        let mut adjusted_history_pcm = history_pcm.clone();
        if total_len > max_samples {
            // Calculate how much to remove from history_pcm
            let excess = total_len - max_samples;
            println!(
                "history_pcm len: {} buffered_pcm len: {} excess: {}",
                history_pcm.len(),
                buffered_pcm.len(),
                excess
            );
            // If history_pcm length exceeds excess, keep the latter part
            if history_pcm.len() > excess {
                adjusted_history_pcm = history_pcm[excess..].to_vec();
            } else {
                // If history_pcm is insufficient, don't use it
                adjusted_history_pcm = Vec::new();
            }
        }

        // Merge adjusted history data and new data for processing
        let mut combined_pcm = Vec::with_capacity(adjusted_history_pcm.len() + buffered_pcm.len());
        combined_pcm.extend_from_slice(&adjusted_history_pcm);
        combined_pcm.extend_from_slice(&buffered_pcm);

        // Update history data to current combined data
        history_pcm = combined_pcm.clone();

        // Clear buffer
        buffered_pcm.clear();

        let pcm = combined_pcm;

        let mel = audio::pcm_to_mel(&config, &pcm, &mel_filters);
        let mel_len = mel.len();
        let mel = candle_core::Tensor::from_vec(
            mel,
            (1, config.num_mel_bins, mel_len / config.num_mel_bins),
            &device,
        )?;

        if !language_token_set {
            let language_token = match (arg_is_multilingual, arg_language.clone()) {
                (true, None) => Some(multilingual::detect_language(
                    decoder.model(),
                    &tokenizer,
                    &mel,
                )?),
                (false, None) => None,
                (true, Some(language)) => {
                    match model::token_id(&tokenizer, &format!("<|{language}|>")) {
                        Ok(token_id) => Some(token_id),
                        Err(_) => anyhow::bail!("language {language} is not supported"),
                    }
                }
                (false, Some(_)) => {
                    anyhow::bail!("a language cannot be set for non-multilingual models")
                }
            };
            decoder.set_language_token(language_token);
            language_token_set = true;
            language_token_name = match language_token {
                Some(token) => model::get_token_name_by_id(&tokenizer, token),
                None => None,
            };
            println!(
                "language_token: {:?} language_name: {:?}",
                language_token, language_token_name
            );
        }

        // Run the decoder and get results
        let mut segments = decoder.run(
            &mel,
            None,
            inference_timeout.or(Some(inference_interval)),
            max_tokens_per_segment,
            fixed_temperature,
        )?;
        // Calculate and output inference duration
        let inference_duration = inference_start.elapsed();

        let audio_duration = (pcm.len() as f32 / 16000.0 * 1000.0) as u128;

        for segment in &mut segments {
            segment.reasoning_duration = Some(inference_duration.as_millis());
            segment.reasoning_lang = language_token_name.clone();
            segment.audio_duration = Some(audio_duration);
        }

        // Send results and update inference time
        result_callback(segments);
        decoder.reset_kv_cache();
        last_inference_time = now;
    }

    println!("Transcription cancelled");
    result_callback(_make_status_response(model::WhisperStatus::Exit));

    println!("Whisper Exit");
    Ok(())
}

fn _make_status_response(status: model::WhisperStatus) -> Vec<Segment> {
    vec![Segment {
        start: 0.0,
        duration: 0.0,
        dr: model::DecodingResult {
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

pub fn get_mel_bytes(num_mel_bins: usize) -> anyhow::Result<Vec<u8>> {
    let mel_bytes = match num_mel_bins {
        80 => include_bytes!("assets/whisper/melfilters.bytes").as_slice(),
        128 => include_bytes!("assets/whisper/melfilters128.bytes").as_slice(),
        nmel => anyhow::bail!("unexpected num_mel_bins {nmel}"),
    };
    Ok(mel_bytes.to_vec())
}
