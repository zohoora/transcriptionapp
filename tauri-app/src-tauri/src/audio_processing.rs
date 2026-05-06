//! Shared audio processing utilities for batch audio pipelines.
//!
//! Used by both the `process_mobile` CLI and the desktop audio upload command.
//! Provides ffmpeg transcoding, WAV sample reading, and encounter splitting.

use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::OnceLock;

use tracing::info;

/// Supported audio file extensions for upload processing.
pub const SUPPORTED_EXTENSIONS: &[&str] = &[
    "mp3", "wav", "m4a", "aac", "flac", "ogg", "wma", "webm",
];

/// Common ffmpeg locations on macOS (app bundles don't inherit shell PATH).
const FFMPEG_SEARCH_PATHS: &[&str] = &[
    "ffmpeg",                     // system PATH
    "/opt/homebrew/bin/ffmpeg",   // Apple Silicon Homebrew
    "/usr/local/bin/ffmpeg",      // Intel Homebrew
];

/// Cached resolved ffmpeg path (resolved once, reused for all calls).
static FFMPEG_PATH: OnceLock<String> = OnceLock::new();

/// Resolve the ffmpeg binary path, checking common locations.
fn resolve_ffmpeg() -> Result<&'static str, String> {
    let path = FFMPEG_PATH.get_or_init(|| {
        for candidate in FFMPEG_SEARCH_PATHS {
            if Command::new(candidate)
                .arg("-version")
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status()
                .is_ok()
            {
                info!("Resolved ffmpeg at: {candidate}");
                return candidate.to_string();
            }
        }
        String::new() // empty = not found
    });
    if path.is_empty() {
        Err("ffmpeg is required but not found. Install with: brew install ffmpeg".to_string())
    } else {
        Ok(path.as_str())
    }
}

/// Check if ffmpeg is available (searches common paths).
pub fn check_ffmpeg_available() -> Result<(), String> {
    resolve_ffmpeg()?;
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
    let ffmpeg = resolve_ffmpeg()?;
    let status = Command::new(ffmpeg)
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    // ── is_supported_format ──────────────────────────────────────────────

    #[test]
    fn is_supported_format_accepts_every_listed_extension() {
        for ext in SUPPORTED_EXTENSIONS {
            let p = PathBuf::from(format!("/tmp/recording.{ext}"));
            assert!(
                is_supported_format(&p),
                "expected {ext} to be supported"
            );
        }
    }

    #[test]
    fn is_supported_format_is_case_insensitive() {
        assert!(is_supported_format(Path::new("/tmp/A.MP3")));
        assert!(is_supported_format(Path::new("/tmp/A.Wav")));
        assert!(is_supported_format(Path::new("/tmp/A.WEBM")));
    }

    #[test]
    fn is_supported_format_rejects_unsupported() {
        assert!(!is_supported_format(Path::new("/tmp/notes.txt")));
        assert!(!is_supported_format(Path::new("/tmp/video.mp4")));
        assert!(!is_supported_format(Path::new("/tmp/data.bin")));
    }

    #[test]
    fn is_supported_format_rejects_missing_extension() {
        assert!(!is_supported_format(Path::new("/tmp/no_extension")));
        assert!(!is_supported_format(Path::new("/tmp/")));
        assert!(!is_supported_format(Path::new("")));
    }

    // ── split_transcript_into_encounters ─────────────────────────────────

    #[test]
    fn split_transcript_returns_single_encounter_for_empty() {
        let out = split_transcript_into_encounters("");
        assert_eq!(out.len(), 1);
        assert_eq!(out[0], "");
    }

    #[test]
    fn split_transcript_returns_single_encounter_for_short_text() {
        let out = split_transcript_into_encounters("hello doctor");
        assert_eq!(out, vec!["hello doctor".to_string()]);
    }

    #[test]
    fn split_transcript_returns_single_encounter_for_long_text() {
        // V1 contract: never splits, even for long transcripts. Manual split
        // is provided through the desktop history window.
        let long = "word ".repeat(10_000);
        let out = split_transcript_into_encounters(&long);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0], long);
    }

    #[test]
    fn split_transcript_preserves_whitespace_only_input() {
        let out = split_transcript_into_encounters("   \n\t  ");
        assert_eq!(out, vec!["   \n\t  ".to_string()]);
    }

    // ── read_wav_samples ─────────────────────────────────────────────────

    fn write_wav_int16(path: &Path, samples: &[i16]) {
        let spec = hound::WavSpec {
            channels: 1,
            sample_rate: 16_000,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        let mut writer = hound::WavWriter::create(path, spec).unwrap();
        for s in samples {
            writer.write_sample(*s).unwrap();
        }
        writer.finalize().unwrap();
    }

    fn write_wav_float32(path: &Path, samples: &[f32]) {
        let spec = hound::WavSpec {
            channels: 1,
            sample_rate: 16_000,
            bits_per_sample: 32,
            sample_format: hound::SampleFormat::Float,
        };
        let mut writer = hound::WavWriter::create(path, spec).unwrap();
        for s in samples {
            writer.write_sample(*s).unwrap();
        }
        writer.finalize().unwrap();
    }

    fn write_wav_stereo_int16(path: &Path) {
        let spec = hound::WavSpec {
            channels: 2,
            sample_rate: 16_000,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        let mut writer = hound::WavWriter::create(path, spec).unwrap();
        writer.write_sample(0_i16).unwrap();
        writer.write_sample(0_i16).unwrap();
        writer.finalize().unwrap();
    }

    #[test]
    fn read_wav_samples_normalizes_int16_to_unit_range() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("int16.wav");
        write_wav_int16(&path, &[i16::MAX, 0, i16::MIN, 16_384]);

        let samples = read_wav_samples(&path).expect("should read");
        assert_eq!(samples.len(), 4);
        // i16::MAX (32767) / 32768.0 ≈ 0.99997
        assert!((samples[0] - 0.999_969_5).abs() < 1e-5);
        assert_eq!(samples[1], 0.0);
        assert_eq!(samples[2], -1.0);
        // 16384 / 32768 = 0.5
        assert!((samples[3] - 0.5).abs() < 1e-6);
    }

    #[test]
    fn read_wav_samples_returns_float32_unchanged() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("float32.wav");
        let inputs = [0.0_f32, 0.25, -0.75, 1.0, -1.0];
        write_wav_float32(&path, &inputs);

        let samples = read_wav_samples(&path).expect("should read");
        assert_eq!(samples.len(), inputs.len());
        for (got, expected) in samples.iter().zip(inputs.iter()) {
            assert!((got - expected).abs() < 1e-6);
        }
    }

    #[test]
    fn read_wav_samples_rejects_stereo() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("stereo.wav");
        write_wav_stereo_int16(&path);

        let err = read_wav_samples(&path).expect_err("stereo should error");
        assert!(
            err.contains("Expected mono WAV"),
            "expected mono error, got: {err}"
        );
        assert!(err.contains("2 channels"));
    }

    #[test]
    fn read_wav_samples_errors_on_missing_file() {
        let path = Path::new("/tmp/this-file-definitely-does-not-exist-9f7e4c2.wav");
        let err = read_wav_samples(path).expect_err("missing file should error");
        assert!(err.contains("Failed to open WAV"));
    }
}
