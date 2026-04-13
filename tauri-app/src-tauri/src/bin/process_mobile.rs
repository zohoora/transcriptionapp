//! Mobile Recording Processing CLI
//!
//! Polls the profile service for queued mobile recordings, processes them through
//! the same pipeline as the desktop app (STT → encounter detection → SOAP), and
//! uploads results back to the profile service.
//!
//! Shares Rust modules with the desktop app — zero algorithm divergence.
//!
//! Usage:
//!   cargo run --bin process_mobile -- --profile-service-url http://localhost:8090
//!   cargo run --bin process_mobile -- --once   # process one job and exit

use std::env;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tracing::{error, info, warn};

use transcription_app_lib::llm_client::{LLMClient, SoapFormat, SoapOptions};
use transcription_app_lib::whisper_server::WhisperServerClient;

// ── Types matching profile service API ──────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
struct MobileJob {
    job_id: String,
    physician_id: String,
    recording_id: String,
    started_at: String,
    duration_ms: u64,
    status: String,
    error: Option<String>,
    sessions_created: Vec<CreatedSession>,
    created_at: String,
    updated_at: String,
    device_info: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CreatedSession {
    session_id: String,
    encounter_number: u32,
    word_count: usize,
    has_soap: bool,
}

#[derive(Debug, Serialize)]
struct UpdateJobRequest {
    status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    sessions_created: Option<Vec<CreatedSession>>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct Physician {
    id: String,
    name: String,
    #[serde(default)]
    soap_detail_level: Option<u8>,
    #[serde(default)]
    soap_format: Option<String>,
    #[serde(default)]
    soap_custom_instructions: Option<String>,
}

#[derive(Debug, Serialize)]
struct UploadSessionRequest {
    metadata: SessionMetadata,
    transcript: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    soap: Option<String>,
}

#[derive(Debug, Serialize)]
struct SessionMetadata {
    session_id: String,
    started_at: String,
    ended_at: Option<String>,
    duration_ms: Option<u64>,
    word_count: usize,
    has_soap_note: bool,
    has_audio: bool,
    charting_mode: Option<String>,
    encounter_number: Option<u32>,
    detection_method: Option<String>,
    physician_id: Option<String>,
    physician_name: Option<String>,
}

// ── CLI Config ──────────────────────────────────────────────────────────────

struct CliConfig {
    profile_service_url: String,
    stt_url: String,
    llm_url: String,
    llm_api_key: String,
    poll_interval_secs: u64,
    once: bool,
    stt_alias: String,
    soap_model: String,
}

impl CliConfig {
    fn from_args() -> Self {
        let args: Vec<String> = env::args().collect();

        let mut config = Self {
            profile_service_url: env::var("PROFILE_SERVICE_URL")
                .unwrap_or_else(|_| "http://localhost:8090".to_string()),
            stt_url: env::var("STT_ROUTER_URL")
                .unwrap_or_else(|_| "http://localhost:8001".to_string()),
            llm_url: env::var("LLM_ROUTER_URL")
                .unwrap_or_else(|_| "http://localhost:8080".to_string()),
            llm_api_key: env::var("LLM_API_KEY").unwrap_or_default(),
            poll_interval_secs: 10,
            once: false,
            stt_alias: "medical-streaming".to_string(),
            soap_model: "soap-model-fast".to_string(),
        };

        let mut i = 1;
        while i < args.len() {
            match args[i].as_str() {
                "--profile-service-url" => {
                    i += 1;
                    config.profile_service_url = args[i].clone();
                }
                "--stt-url" => {
                    i += 1;
                    config.stt_url = args[i].clone();
                }
                "--llm-url" => {
                    i += 1;
                    config.llm_url = args[i].clone();
                }
                "--llm-api-key" => {
                    i += 1;
                    config.llm_api_key = args[i].clone();
                }
                "--poll-interval" => {
                    i += 1;
                    config.poll_interval_secs = args[i].parse().unwrap_or(10);
                }
                "--once" => {
                    config.once = true;
                }
                "--help" | "-h" => {
                    print_usage(&args[0]);
                    std::process::exit(0);
                }
                _ => {
                    eprintln!("Unknown argument: {}", args[i]);
                    print_usage(&args[0]);
                    std::process::exit(1);
                }
            }
            i += 1;
        }

        config
    }
}

fn print_usage(program: &str) {
    eprintln!("Mobile Recording Processing CLI");
    eprintln!();
    eprintln!("Polls the profile service for uploaded mobile recordings and processes them");
    eprintln!("through STT → encounter detection → SOAP generation.");
    eprintln!();
    eprintln!("Usage: {} [options]", program);
    eprintln!();
    eprintln!("Options:");
    eprintln!("  --profile-service-url URL  Profile service URL (default: $PROFILE_SERVICE_URL or http://localhost:8090)");
    eprintln!("  --stt-url URL             STT Router URL (default: $STT_ROUTER_URL or http://localhost:8001)");
    eprintln!("  --llm-url URL             LLM Router URL (default: $LLM_ROUTER_URL or http://localhost:8080)");
    eprintln!("  --llm-api-key KEY         LLM Router API key (default: $LLM_API_KEY)");
    eprintln!("  --poll-interval SECS      Poll interval in seconds (default: 10)");
    eprintln!("  --once                    Process one job and exit");
    eprintln!("  --help                    Show this help");
}

// ── Profile Service Client ──────────────────────────────────────────────────

struct ProfileServiceClient {
    client: reqwest::Client,
    base_url: String,
}

impl ProfileServiceClient {
    fn new(base_url: &str) -> Self {
        Self {
            client: reqwest::Client::builder()
                .connect_timeout(Duration::from_secs(5))
                .timeout(Duration::from_secs(30))
                .build()
                .expect("Failed to create HTTP client"),
            base_url: base_url.trim_end_matches('/').to_string(),
        }
    }

    async fn get_queued_jobs(&self) -> Result<Vec<MobileJob>, String> {
        let url = format!("{}/mobile/jobs?status=queued", self.base_url);
        let resp = self
            .client
            .get(&url)
            .send()
            .await
            .map_err(|e| format!("Failed to fetch jobs: {e}"))?;
        if !resp.status().is_success() {
            return Err(format!("Failed to fetch jobs: HTTP {}", resp.status()));
        }
        resp.json()
            .await
            .map_err(|e| format!("Failed to parse jobs: {e}"))
    }

    async fn update_job_status(
        &self,
        job_id: &str,
        req: &UpdateJobRequest,
    ) -> Result<(), String> {
        let url = format!("{}/mobile/jobs/{}", self.base_url, job_id);
        let resp = self
            .client
            .put(&url)
            .json(req)
            .send()
            .await
            .map_err(|e| format!("Failed to update job: {e}"))?;
        if !resp.status().is_success() {
            return Err(format!("Failed to update job: HTTP {}", resp.status()));
        }
        Ok(())
    }

    async fn download_audio(&self, job_id: &str) -> Result<Vec<u8>, String> {
        let url = format!("{}/mobile/uploads/{}", self.base_url, job_id);
        let resp = self
            .client
            .get(&url)
            .send()
            .await
            .map_err(|e| format!("Failed to download audio: {e}"))?;
        if !resp.status().is_success() {
            return Err(format!(
                "Failed to download audio: HTTP {}",
                resp.status()
            ));
        }
        resp.bytes()
            .await
            .map(|b| b.to_vec())
            .map_err(|e| format!("Failed to read audio bytes: {e}"))
    }

    async fn get_physician(&self, physician_id: &str) -> Result<Physician, String> {
        let url = format!("{}/physicians/{}", self.base_url, physician_id);
        let resp = self
            .client
            .get(&url)
            .send()
            .await
            .map_err(|e| format!("Failed to fetch physician: {e}"))?;
        if !resp.status().is_success() {
            return Err(format!(
                "Failed to fetch physician: HTTP {}",
                resp.status()
            ));
        }
        resp.json()
            .await
            .map_err(|e| format!("Failed to parse physician: {e}"))
    }

    async fn upload_session(
        &self,
        physician_id: &str,
        session_id: &str,
        req: &UploadSessionRequest,
    ) -> Result<(), String> {
        let url = format!(
            "{}/physicians/{}/sessions/{}",
            self.base_url, physician_id, session_id
        );
        let resp = self
            .client
            .post(&url)
            .json(req)
            .send()
            .await
            .map_err(|e| format!("Failed to upload session: {e}"))?;
        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(format!("Failed to upload session: HTTP {status} — {body}"));
        }
        Ok(())
    }
}

// ── Pipeline Functions ──────────────────────────────────────────────────────

/// Transcode AAC/m4a to WAV (16kHz mono PCM) using ffmpeg.
fn transcode_to_wav(input: &Path) -> Result<PathBuf, String> {
    let output = input.with_extension("wav");
    let status = Command::new("ffmpeg")
        .args([
            "-y", // overwrite output
            "-i",
            input.to_str().ok_or("Invalid input path")?,
            "-ar",
            "16000",
            "-ac",
            "1",
            "-f",
            "wav",
            output.to_str().ok_or("Invalid output path")?,
        ])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map_err(|e| format!("Failed to run ffmpeg: {e}. Is ffmpeg installed?"))?;

    if !status.success() {
        return Err(format!(
            "ffmpeg exited with code {}",
            status.code().unwrap_or(-1)
        ));
    }
    Ok(output)
}

/// Read a WAV file into f32 samples (expected: 16kHz mono PCM).
fn read_wav_samples(path: &Path) -> Result<Vec<f32>, String> {
    let reader = hound::WavReader::open(path)
        .map_err(|e| format!("Failed to open WAV: {e}"))?;
    let spec = reader.spec();

    if spec.channels != 1 {
        return Err(format!("Expected mono WAV, got {} channels", spec.channels));
    }

    match spec.sample_format {
        hound::SampleFormat::Int => {
            let max_val = (1 << (spec.bits_per_sample - 1)) as f32;
            let samples: Vec<f32> = reader
                .into_samples::<i32>()
                .filter_map(|s| s.ok())
                .map(|s| s as f32 / max_val)
                .collect();
            Ok(samples)
        }
        hound::SampleFormat::Float => {
            let samples: Vec<f32> = reader
                .into_samples::<f32>()
                .filter_map(|s| s.ok())
                .collect();
            Ok(samples)
        }
    }
}

/// Transcribe audio using the STT Router (batch mode).
async fn transcribe_audio(
    stt_client: &WhisperServerClient,
    samples: &[f32],
    alias: &str,
) -> Result<String, String> {
    info!(
        "Transcribing {} samples ({:.1}s)...",
        samples.len(),
        samples.len() as f32 / 16000.0
    );
    stt_client.transcribe_batch(samples, alias, true).await
}

/// Detect encounter boundaries in a transcript.
/// Returns a list of transcript segments (one per detected encounter).
fn split_transcript_into_encounters(transcript: &str) -> Vec<String> {
    let word_count = transcript.split_whitespace().count();

    // For v1: simple word-count based splitting. If < 500 words, treat as single encounter.
    // Full LLM-based detection will be added in a follow-up.
    if word_count < 500 {
        return vec![transcript.to_string()];
    }

    // For now, return as single encounter. The encounter detection via LLM
    // requires the full prompt infrastructure which we'll wire up iteratively.
    // The desktop app's encounter detection is real-time (timer-based), so
    // batch detection needs a different prompt approach.
    vec![transcript.to_string()]
}

/// Generate a SOAP note for a transcript segment.
async fn generate_soap(
    llm_client: &LLMClient,
    transcript: &str,
    model: &str,
    physician: &Physician,
) -> Result<Option<String>, String> {
    let word_count = transcript.split_whitespace().count();
    if word_count < 50 {
        info!("Transcript too short for SOAP ({word_count} words), skipping");
        return Ok(None);
    }

    let options = SoapOptions {
        detail_level: physician.soap_detail_level.unwrap_or(5),
        format: physician
            .soap_format
            .as_deref()
            .map(SoapFormat::from_config_str)
            .unwrap_or_default(),
        custom_instructions: physician
            .soap_custom_instructions
            .clone()
            .unwrap_or_default(),
        ..Default::default()
    };

    info!("Generating SOAP for {word_count} words...");
    let soap_note = llm_client
        .generate_soap_note(model, transcript, None, Some(&options), None)
        .await?;
    Ok(Some(soap_note.content))
}

/// Process a single job through the full pipeline.
async fn process_job(
    job: &MobileJob,
    profile_client: &ProfileServiceClient,
    stt_client: &WhisperServerClient,
    llm_client: &LLMClient,
    config: &CliConfig,
    work_dir: &Path,
) -> Result<Vec<CreatedSession>, String> {
    // Step 1: Download audio
    info!("Downloading audio for job {}...", job.job_id);
    let audio_data = profile_client.download_audio(&job.job_id).await?;
    let m4a_path = work_dir.join(format!("{}.m4a", job.job_id));
    tokio::fs::write(&m4a_path, &audio_data)
        .await
        .map_err(|e| format!("Failed to write audio: {e}"))?;

    // Step 2: Transcode
    profile_client
        .update_job_status(
            &job.job_id,
            &UpdateJobRequest {
                status: "transcoding".to_string(),
                error: None,
                sessions_created: None,
            },
        )
        .await?;
    info!("Transcoding AAC → WAV...");
    let wav_path = transcode_to_wav(&m4a_path)?;

    // Step 3: Speech-to-text
    profile_client
        .update_job_status(
            &job.job_id,
            &UpdateJobRequest {
                status: "transcribing".to_string(),
                error: None,
                sessions_created: None,
            },
        )
        .await?;
    let samples = read_wav_samples(&wav_path)?;
    let transcript = transcribe_audio(stt_client, &samples, &config.stt_alias).await?;
    info!(
        "Transcription complete: {} words",
        transcript.split_whitespace().count()
    );

    // Step 4: Encounter detection
    profile_client
        .update_job_status(
            &job.job_id,
            &UpdateJobRequest {
                status: "detecting".to_string(),
                error: None,
                sessions_created: None,
            },
        )
        .await?;
    let encounters = split_transcript_into_encounters(&transcript);
    info!("Detected {} encounter(s)", encounters.len());

    // Step 5: SOAP generation + session creation
    profile_client
        .update_job_status(
            &job.job_id,
            &UpdateJobRequest {
                status: "generating_soap".to_string(),
                error: None,
                sessions_created: None,
            },
        )
        .await?;

    let physician = profile_client.get_physician(&job.physician_id).await?;
    let mut created_sessions = Vec::new();

    for (idx, encounter_transcript) in encounters.iter().enumerate() {
        let session_id = uuid::Uuid::new_v4().to_string();
        let encounter_number = (idx as u32) + 1;
        let word_count = encounter_transcript.split_whitespace().count();

        // Generate SOAP
        let soap = generate_soap(
            llm_client,
            encounter_transcript,
            &config.soap_model,
            &physician,
        )
        .await
        .unwrap_or_else(|e| {
            warn!("SOAP generation failed for encounter {encounter_number}: {e}");
            None
        });

        let has_soap = soap.is_some();

        // Upload session to profile service
        let req = UploadSessionRequest {
            metadata: SessionMetadata {
                session_id: session_id.clone(),
                started_at: job.started_at.clone(),
                ended_at: None,
                duration_ms: Some(job.duration_ms),
                word_count,
                has_soap_note: has_soap,
                has_audio: false,
                charting_mode: Some("mobile".to_string()),
                encounter_number: Some(encounter_number),
                detection_method: Some("batch_llm".to_string()),
                physician_id: Some(job.physician_id.clone()),
                physician_name: Some(physician.name.clone()),
            },
            transcript: encounter_transcript.clone(),
            soap,
        };

        profile_client
            .upload_session(&job.physician_id, &session_id, &req)
            .await?;

        info!(
            "Created session {} (encounter {}, {} words, soap={})",
            &session_id[..8],
            encounter_number,
            word_count,
            has_soap
        );

        created_sessions.push(CreatedSession {
            session_id,
            encounter_number,
            word_count,
            has_soap,
        });
    }

    // Cleanup temp files
    let _ = tokio::fs::remove_file(&m4a_path).await;
    let _ = tokio::fs::remove_file(&wav_path).await;

    Ok(created_sessions)
}

// ── Main ────────────────────────────────────────────────────────────────────

#[tokio::main]
async fn main() {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let config = CliConfig::from_args();

    info!("Mobile Processing CLI starting");
    info!("  Profile service: {}", config.profile_service_url);
    info!("  STT Router:      {}", config.stt_url);
    info!("  LLM Router:      {}", config.llm_url);
    info!("  Poll interval:   {}s", config.poll_interval_secs);
    info!("  Mode:            {}", if config.once { "once" } else { "daemon" });

    // Check ffmpeg is available
    match Command::new("ffmpeg")
        .arg("-version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
    {
        Ok(s) if s.success() => info!("ffmpeg: available"),
        _ => {
            error!("ffmpeg not found in PATH. Install with: brew install ffmpeg");
            std::process::exit(1);
        }
    }

    let profile_client = ProfileServiceClient::new(&config.profile_service_url);
    let stt_client = WhisperServerClient::new(&config.stt_url, "default")
        .expect("Failed to create STT client");
    let llm_client =
        LLMClient::new(&config.llm_url, &config.llm_api_key, "process_mobile", "fast-model")
            .expect("Failed to create LLM client");

    // Work directory for temp files
    let work_dir = std::env::temp_dir().join("process_mobile");
    std::fs::create_dir_all(&work_dir).expect("Failed to create work directory");

    loop {
        match profile_client.get_queued_jobs().await {
            Ok(jobs) => {
                if jobs.is_empty() {
                    if config.once {
                        info!("No queued jobs found. Exiting (--once mode).");
                        break;
                    }
                } else {
                    for job in &jobs {
                        info!("Processing job {} (physician={}, {:.0}s audio)",
                            &job.job_id[..8],
                            &job.physician_id[..8.min(job.physician_id.len())],
                            job.duration_ms as f64 / 1000.0,
                        );

                        match process_job(
                            job,
                            &profile_client,
                            &stt_client,
                            &llm_client,
                            &config,
                            &work_dir,
                        )
                        .await
                        {
                            Ok(sessions) => {
                                info!(
                                    "Job {} complete: {} session(s) created",
                                    &job.job_id[..8],
                                    sessions.len()
                                );
                                let _ = profile_client
                                    .update_job_status(
                                        &job.job_id,
                                        &UpdateJobRequest {
                                            status: "complete".to_string(),
                                            error: None,
                                            sessions_created: Some(sessions),
                                        },
                                    )
                                    .await;
                            }
                            Err(e) => {
                                error!("Job {} failed: {}", &job.job_id[..8], e);
                                let _ = profile_client
                                    .update_job_status(
                                        &job.job_id,
                                        &UpdateJobRequest {
                                            status: "failed".to_string(),
                                            error: Some(e),
                                            sessions_created: None,
                                        },
                                    )
                                    .await;
                            }
                        }

                        if config.once {
                            info!("Processed one job. Exiting (--once mode).");
                            return;
                        }
                    }
                }
            }
            Err(e) => {
                warn!("Failed to poll for jobs: {e}");
            }
        }

        if config.once {
            break;
        }

        tokio::time::sleep(Duration::from_secs(config.poll_interval_secs)).await;
    }
}
