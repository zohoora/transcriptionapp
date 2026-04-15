//! Shared audio processing utilities for batch audio pipelines.
//!
//! Used by both the `process_mobile` CLI and the desktop audio upload command.
//! Provides ffmpeg transcoding, WAV sample reading, and encounter splitting.

use std::path::Path;
use std::process::{Command, Stdio};

use tracing::info;

/// Supported audio file extensions for upload processing.
pub const SUPPORTED_EXTENSIONS: &[&str] = &[
    "mp3", "wav", "m4a", "aac", "flac", "ogg", "wma", "webm",
];

/// Check if ffmpeg is available in PATH.
pub fn check_ffmpeg_available() -> Result<(), String> {
    Command::new("ffmpeg")
        .arg("-version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map_err(|_| "ffmpeg is required but not found. Install with: brew install ffmpeg".to_string())?;
    Ok(())
}

/// Check if a file extension is a supported audio format.
pub fn is_supported_format(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| SUPPORTED_EXTENSIONS.contains(&ext.to_lowercase().as_str()))
        .unwrap_or(false)
}

/// Transcode any supported audio format to WAV (16kHz mono PCM) using ffmpeg.
pub fn transcode_to_wav(input: &Path, output: &Path) -> Result<(), String> {
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
    Ok(())
}

/// Read a WAV file into f32 samples (expected: 16kHz mono PCM).
pub fn read_wav_samples(path: &Path) -> Result<Vec<f32>, String> {
    let reader =
        hound::WavReader::open(path).map_err(|e| format!("Failed to open WAV: {e}"))?;
    let spec = reader.spec();

    if spec.channels != 1 {
        return Err(format!(
            "Expected mono WAV, got {} channels",
            spec.channels
        ));
    }

    match spec.sample_format {
        hound::SampleFormat::Int => {
            let max_val = (1_i64 << (spec.bits_per_sample - 1)) as f32;
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

/// Detect encounter boundaries in a transcript.
/// Returns a list of transcript segments (one per detected encounter).
///
/// V1: simple word-count heuristic. All transcripts are returned as a single encounter.
/// Full LLM-based batch detection will be added in a follow-up.
pub fn split_transcript_into_encounters(transcript: &str) -> Vec<String> {
    let word_count = transcript.split_whitespace().count();
    info!("Splitting transcript ({word_count} words) into encounters");

    // V1: return as single encounter regardless of length.
    // The desktop History window has a manual split tool for users who need it.
    vec![transcript.to_string()]
}
