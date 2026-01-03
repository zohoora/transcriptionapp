# Local Transcription App Specification v3.3 (Final)

> **HISTORICAL DOCUMENT**: This is the original POC specification from December 2024. The project has since been completed and expanded with additional features (SOAP notes, EMR integration, biomarkers, etc.). For current documentation, see [tauri-app/CLAUDE.md](../tauri-app/CLAUDE.md).

> **Purpose:** A desktop transcription app that records from the microphone, transcribes offline in near-real-time, and copies the final transcript to clipboard.
> **Target:** AI-assisted development with comprehensive automated testing.
> **Architecture:** Designed for future layering (SOAP notes, summaries, structured extraction).
> **Status:** ✅ COMPLETE - All milestones implemented

---

## 0. Scope Definition

### 0.1 POC Scope (What We Build First)

The POC proves the core loop works reliably. Everything else is deferred.

**POC Must Prove:**
- Microphone capture works across input devices
- Offline transcription works (Whisper only)
- Finalized transcript updates within 1–4 seconds of utterance end
- Copy to clipboard works
- VAD prevents hallucinations during silence
- Timestamps remain accurate despite VAD gating

**POC Explicitly Defers:**
- Apple Speech provider (keep interface stub only)
- Multilingual UI (support `auto` + `en` only; keep `language` in settings model)
- In-app model downloader (manual model placement)
- Crash recovery (optional enhancement)
- WER-based golden tests (flaky across models/CPUs)
- Windows build (architect for it, don't block on it)

**POC Success Criteria:**
- Records and transcribes a 5-minute session without crashes
- Finalized transcript updates within 1–4 seconds of each utterance end
- No hallucinations during 10+ seconds of silence
- Timestamps accurate to ±500ms despite silence gaps
- Copy produces clean, usable text
- All automated tests pass in CI

### 0.2 Confirmed Requirements

1. **Platforms:** macOS first. Windows architecturally supported, shipped in Milestone 5.
2. **Offline:** Fully offline—no network calls, no analytics, no telemetry, no auto-updater.
3. **Languages:** English-only for POC. Model supports multilingual; UI deferred.
4. **Audio:** Selectable input devices.
5. **Output:** Copy-to-clipboard only.

---

## 1. Architecture Overview

### 1.1 High-Level Design

```
┌─────────────────────────────────────────────────────────────────────────┐
│                              Tauri App                                   │
├─────────────────────────────────────────────────────────────────────────┤
│  ┌─────────────────┐      IPC       ┌─────────────────────────────────┐ │
│  │   React/TS UI   │◄──────────────►│          Rust Backend           │ │
│  │                 │    Events      │                                 │ │
│  │  - Session View │                │  ┌───────────────────────────┐  │ │
│  │  - Settings     │                │  │    Session Controller     │  │ │
│  │  - Transcript   │                │  │    (State Machine)        │  │ │
│  │  - Copy Button  │                │  └─────────────┬─────────────┘  │ │
│  └─────────────────┘                │                │                │ │
│                                     │  ┌─────────────▼─────────────┐  │ │
│                                     │  │      Audio Pipeline       │  │ │
│                                     │  │  (See Section 2.3)        │  │ │
│                                     │  └─────────────┬─────────────┘  │ │
│                                     │                │                │ │
│                                     │  ┌─────────────▼─────────────┐  │ │
│                                     │  │    Provider Interface     │  │ │
│                                     │  │  - WhisperProvider ✓      │  │ │
│                                     │  │  - AppleProvider (stub)   │  │ │
│                                     │  └─────────────┬─────────────┘  │ │
│                                     │                │                │ │
│                                     │  ┌─────────────▼─────────────┐  │ │
│                                     │  │   Transcript Assembler    │  │ │
│                                     │  │   (Incremental Strategy)  │  │ │
│                                     │  └───────────────────────────┘  │ │
│                                     └─────────────────────────────────┘ │
└─────────────────────────────────────────────────────────────────────────┘
```

### 1.2 Core Technology Stack

| Component | Technology | Rationale |
|-----------|------------|-----------|
| Desktop Shell | Tauri 2.x | Single binary, Rust core, cross-platform |
| UI | React + TypeScript + Vite | Fast iteration, type safety |
| Audio Capture | `cpal` crate | Cross-platform, well-maintained |
| Ring Buffer | `ringbuf` crate | Lock-free SPSC, real-time safe |
| Resampling | `rubato` crate | High-quality, realtime-safe |
| VAD | `voice_activity_detector` crate | Silero VAD, clean Rust API |
| Whisper | `whisper-rs` crate | Mature bindings to whisper.cpp |
| State Management | Rust state machine | Explicit, testable |

---

## 2. Critical Architecture Decisions

### 2.1 Voice Activity Detection (VAD) — MANDATORY

**Problem:** Whisper hallucinates during silence, producing outputs like "Thank you for watching," "Subtitles by," or repeating the last phrase.

**Solution:** Silero VAD controls when inference runs—but does NOT drop audio bytes from the timeline.

#### 2.1.1 VAD Gating Behavior (Precise Specification)

**Critical Rule:** VAD controls *inference*, not *audio storage*. The audio clock always advances with processed samples.

```
┌─────────────────────────────────────────────────────────────────────────┐
│                          VAD Gating Logic                                │
├─────────────────────────────────────────────────────────────────────────┤
│                                                                          │
│  Audio In ──► Ring Buffer ──► VAD Check ──┬──► Speech? ──► Accumulate   │
│               (always         (every       │               in utterance  │
│                advances)       chunk)      │               buffer        │
│                    │                       │                             │
│                    │                       └──► Silence? ──► Track       │
│                    │                                        silence      │
│                    │                                        duration     │
│                    ▼                                                     │
│              Timeline Clock                                              │
│              (NEVER paused)                                              │
│                                                                          │
└─────────────────────────────────────────────────────────────────────────┘
```

**VAD Parameters:**

| Parameter | Value | Samples (16kHz) | Purpose |
|-----------|-------|-----------------|---------|
| `vad_threshold` | 0.5 | — | Speech probability threshold |
| `pre_roll_ms` | 300 | 4,800 | Audio before speech (soft consonants) |
| `min_speech_ms` | 250 | 4,000 | Ignore shorter sounds (noise filter) |
| `silence_to_flush_ms` | 500 | 8,000 | Silence that ends an utterance |
| `max_utterance_ms` | 25,000 | 400,000 | Hard limit (Whisper's 30s safety) |

**Implementation note:** Internally, use sample counts (not milliseconds) to avoid drift between audio clock and wall clock when processing falls behind.

**Critical: The "Long Talker" Problem**

Whisper has a hard input limit of ~30 seconds. If a user speaks continuously without pausing (e.g., reading text aloud), the utterance buffer grows unbounded and will either error or hallucinate.

**Solution:** Enforce `max_utterance_ms = 25000` (25 seconds, with safety margin). See full implementation in VadGatedPipeline below.

**Post-Roll vs Silence-to-Flush Clarification:**

These are distinct concepts:
- **`post_roll_ms` (300ms):** How much silence to *include* in the audio sent to Whisper (for trailing sounds)
- **`silence_to_flush_ms` (500ms):** How long to *wait* before deciding the utterance is complete

Implementation: When silence begins, start a timer. Continue accumulating audio (including silence) into the buffer. After `silence_to_flush_ms`, flush the buffer. The buffer will contain speech + up to 500ms of trailing silence, which is fine—Whisper handles trailing silence well.

```rust
struct VadGatedPipeline {
    // Audio clock: count of 16kHz samples processed
    // IMPORTANT: Represents the END of the most recently processed chunk
    // (because advance_audio_clock is called BEFORE process_chunk)
    audio_clock_samples: u64,
    
    // VAD state
    is_speech_active: bool,
    silence_samples: u64,
    
    // Speech accumulator
    speech_buffer: Vec<f32>,
    speech_start_samples: u64,
    
    // Pre-roll buffer (contains samples BEFORE current chunk)
    pre_roll_buffer: VecDeque<f32>,
    
    // Configuration
    config: VadConfig,
    
    // Output queue
    transcription_queue: VecDeque<Utterance>,
}

struct VadConfig {
    vad_threshold: f32,               // 0.5
    pre_roll_samples: usize,          // 4800 (300ms at 16kHz)
    min_speech_samples: usize,        // 4000 (250ms at 16kHz)
    silence_to_flush_samples: usize,  // 8000 (500ms at 16kHz)
    max_utterance_samples: usize,     // 400000 (25s at 16kHz)
}

impl VadGatedPipeline {
    fn advance_audio_clock(&mut self, samples: usize) {
        self.audio_clock_samples += samples as u64;
    }
    
    /// Returns timestamp at START of current chunk
    /// audio_clock is at END, so subtract chunk length
    fn chunk_start_samples(&self, chunk_len: usize) -> u64 {
        self.audio_clock_samples.saturating_sub(chunk_len as u64)
    }
    
    fn process_chunk(&mut self, audio: &[f32], vad: &mut VoiceActivityDetector) {
        let chunk_len = audio.len();
        let chunk_start = self.chunk_start_samples(chunk_len);
        let is_speech = vad.predict(audio).unwrap_or(0.0) > self.config.vad_threshold;
        
        // CRITICAL: Check max utterance length FIRST
        if self.is_speech_active {
            if self.speech_buffer.len() >= self.config.max_utterance_samples {
                self.flush_utterance();
                // Restart immediately (speech still active)
                self.is_speech_active = true;
                // FIX: Subtract pre-roll from chunk start (same rule as normal start)
                self.speech_start_samples = chunk_start
                    .saturating_sub(self.pre_roll_buffer.len() as u64);
                self.speech_buffer.extend(self.pre_roll_buffer.iter());
            }
        }
        
        match (self.is_speech_active, is_speech) {
            // Transition: silence → speech
            (false, true) => {
                self.is_speech_active = true;
                self.silence_samples = 0;
                
                // Start time = chunk start minus pre-roll
                // (chunk_start is already correct for END-based audio clock)
                self.speech_start_samples = chunk_start
                    .saturating_sub(self.pre_roll_buffer.len() as u64);
                
                self.speech_buffer.clear();
                self.speech_buffer.extend(self.pre_roll_buffer.iter());
                self.speech_buffer.extend(audio.iter());
            }
            
            // Continuing speech
            (true, true) => {
                self.speech_buffer.extend(audio.iter());
                self.silence_samples = 0;
            }
            
            // Transition: speech → silence
            (true, false) => {
                self.speech_buffer.extend(audio.iter());
                self.silence_samples += chunk_len as u64;
                
                if self.silence_samples >= self.config.silence_to_flush_samples as u64 {
                    self.flush_utterance();
                }
            }
            
            // Continuing silence
            (false, false) => {
                // Nothing to accumulate
            }
        }
        
        // Update pre-roll buffer AFTER processing
        self.pre_roll_buffer.extend(audio.iter().copied());
        while self.pre_roll_buffer.len() > self.config.pre_roll_samples {
            self.pre_roll_buffer.pop_front();
        }
    }
    
    fn flush_utterance(&mut self) {
        if self.speech_buffer.len() < self.config.min_speech_samples {
            self.speech_buffer.clear();
            self.is_speech_active = false;
            self.silence_samples = 0;
            return;
        }
        
        let start_ms = self.speech_start_samples / 16;
        let end_ms = start_ms + (self.speech_buffer.len() as u64 / 16);
        
        let utterance = Utterance {
            audio: std::mem::take(&mut self.speech_buffer),
            start_ms,
            end_ms,
        };
        
        self.transcription_queue.push_back(utterance);
        self.is_speech_active = false;
        self.silence_samples = 0;
    }
}
```

### 2.2 Audio Pipeline Architecture

**Problem:** `cpal` provides variable buffer sizes on a high-priority callback thread. `rubato` requires fixed chunks and is not realtime-safe. Mixing these incorrectly causes audio glitches or panics.

**Solution:** Lock-free ring buffer stores RAW device-rate audio. All processing (resampling, VAD, Whisper) happens in a separate relaxed thread.

#### 2.2.1 Pipeline Diagram

```
┌─────────────────────────────────────────────────────────────────────────┐
│                         Audio Pipeline Architecture                      │
├─────────────────────────────────────────────────────────────────────────┤
│                                                                          │
│  ┌─────────────────────────────────────────────────────────────────────┐ │
│  │                    CAPTURE THREAD (High Priority)                   │ │
│  │                                                                     │ │
│  │  Microphone ──► Format Convert ──► Ring Buffer (RAW device rate)   │ │
│  │               (i16/u8 → f32)       (lock-free, no allocations)     │ │
│  │                                                                     │ │
│  │  Rules: NO allocations, NO locks, NO blocking calls                │ │
│  └─────────────────────────────────────────────────────────────────────┘ │
│                                    │                                     │
│                                    ▼                                     │
│  ┌─────────────────────────────────────────────────────────────────────┐ │
│  │                   PROCESSING THREAD (Relaxed)                       │ │
│  │                                                                     │ │
│  │  Ring Buffer ──► Read Fixed ──► Resample ──► VAD ──► Whisper       │ │
│  │                  Chunk          (rubato)     Gate    (if speech)   │ │
│  │                  (1024 frames)  (→ 16kHz)                          │ │
│  │                                                                     │ │
│  │  Rules: Can allocate, can block, can take time                     │ │
│  └─────────────────────────────────────────────────────────────────────┘ │
│                                                                          │
└─────────────────────────────────────────────────────────────────────────┘
```

**Critical Design Rules:**
1. **Capture callback is sacred** — no allocations, no locks, no blocking
2. **Ring buffer stores raw audio** — at device sample rate (e.g., 44.1kHz, 48kHz)
3. **Processing thread does heavy lifting** — resampling, VAD, Whisper all happen here
4. **Fixed chunks for rubato** — processing thread reads exactly `resampler.input_frames_next()` samples

#### 2.2.2 Ring Buffer Sizing

The ring buffer stores RAW device-rate audio (not 16kHz).

**Sizing for 48kHz device (common case):**
- **Capacity:** 1,440,000 samples = 30 seconds at 48kHz
- **Memory:** ~5.5MB (acceptable)

**Sizing for 44.1kHz device:**
- **Capacity:** 1,323,000 samples = 30 seconds at 44.1kHz

**Implementation:** Size dynamically based on detected device sample rate:

```rust
fn calculate_ring_buffer_capacity(device_sample_rate: u32) -> usize {
    const BUFFER_DURATION_SECONDS: u32 = 30;
    (device_sample_rate * BUFFER_DURATION_SECONDS) as usize
}
```

#### 2.2.3 Capture Callback (Realtime-Safe)

**Critical:** The callback must be allocation-free and lock-free.

**Channel Handling:** Many devices default to stereo. Downmix to mono before storing.

```rust
use ringbuf::{HeapRb, Producer};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

fn build_input_stream(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    producer: Producer<f32, Arc<HeapRb<f32>>>,
) -> Result<cpal::Stream, AudioError> {
    
    let sample_format = config.sample_format();
    let channels = config.channels as usize;
    let overflow_counter = Arc::new(AtomicU64::new(0));
    let overflow_clone = overflow_counter.clone();
    
    match sample_format {
        cpal::SampleFormat::F32 => {
            device.build_input_stream(
                config,
                move |data: &[f32], _| {
                    // Downmix to mono if stereo (or more channels)
                    if channels == 1 {
                        let pushed = producer.push_slice(data);
                        if pushed < data.len() {
                            overflow_clone.fetch_add(1, Ordering::Relaxed);
                        }
                    } else {
                        // Downmix: take channel 0 only (simple, no allocation)
                        // For better quality, could average channels, but this
                        // requires a pre-allocated buffer
                        for chunk in data.chunks(channels) {
                            if producer.push(chunk[0]).is_err() {
                                overflow_clone.fetch_add(1, Ordering::Relaxed);
                                break;
                            }
                        }
                    }
                },
                error_callback,
                None,
            )
        }
        cpal::SampleFormat::I16 => {
            device.build_input_stream(
                config,
                move |data: &[i16], _| {
                    // Convert and downmix
                    for chunk in data.chunks(channels) {
                        let sample = chunk[0] as f32 / 32768.0;
                        if producer.push(sample).is_err() {
                            overflow_clone.fetch_add(1, Ordering::Relaxed);
                            break;
                        }
                    }
                },
                error_callback,
                None,
            )
        }
        _ => Err(AudioError::UnsupportedFormat(sample_format)),
    }
}
```

**Device Configuration Preference:** When selecting input config, prefer mono if available:

```rust
fn select_input_config(device: &cpal::Device) -> cpal::StreamConfig {
    let supported = device.supported_input_configs().unwrap();
    
    // Prefer: 1 channel, then device default sample rate
    for config in supported {
        if config.channels() == 1 {
            return config.with_max_sample_rate().into();
        }
    }
    
    // Fall back to default (will downmix in callback)
    device.default_input_config().unwrap().into()
}
```

#### 2.2.4 Processing Thread

**VAD Chunk Sizing Problem:** `SincFixedIn` with 1024 input frames at 48kHz produces ~341 samples at 16kHz, but VAD expects exactly 512 samples.

**Solution:** Add a 16kHz staging buffer after resampling. Accumulate until 512 samples, then feed VAD.

```rust
fn processing_thread_main(
    consumer: Consumer<f32, Arc<HeapRb<f32>>>,
    device_sample_rate: u32,
) {
    // Create resampler (device rate → 16kHz)
    let mut resampler = SincFixedIn::<f32>::new(
        16000.0 / device_sample_rate as f64,
        2.0,
        SincInterpolationParameters::default(),
        1024,
        1,
    ).unwrap();
    
    // VAD expects exactly 512 samples at 16kHz
    const VAD_CHUNK_SIZE: usize = 512;
    let mut vad = VoiceActivityDetector::builder()
        .sample_rate(16000)
        .chunk_size(VAD_CHUNK_SIZE)
        .build()
        .unwrap();
    
    // Staging buffer: accumulates 16kHz samples until we have VAD_CHUNK_SIZE
    let mut staging_buffer: Vec<f32> = Vec::with_capacity(VAD_CHUNK_SIZE * 2);
    
    let mut pipeline = VadGatedPipeline::new();
    
    loop {
        let input_frames_needed = resampler.input_frames_next();
        
        // Wait for enough raw samples
        while consumer.len() < input_frames_needed {
            std::thread::sleep(Duration::from_millis(5));
        }
        
        // Read and resample
        let mut input_buffer = vec![0.0f32; input_frames_needed];
        consumer.pop_slice(&mut input_buffer);
        
        let resampled = resampler.process(&[&input_buffer], None).unwrap();
        let resampled_mono = &resampled[0];
        
        // Accumulate into staging buffer
        staging_buffer.extend_from_slice(resampled_mono);
        
        // Process complete VAD chunks
        while staging_buffer.len() >= VAD_CHUNK_SIZE {
            let chunk: Vec<f32> = staging_buffer.drain(..VAD_CHUNK_SIZE).collect();
            
            // Advance audio clock by chunk size (in 16kHz samples)
            pipeline.advance_audio_clock(VAD_CHUNK_SIZE);
            
            // VAD + accumulation
            pipeline.process_chunk(&chunk, &mut vad);
        }
    }
}
```

#### 2.2.5 Backpressure Policy

**Honest policy:** We design to never drop audio in normal operation. Overflow is a visible error condition.

| Condition | Behavior |
|-----------|----------|
| Normal operation | Ring buffer absorbs processing latency |
| Processing behind (queue > 3) | UI shows orange "Processing..." badge |
| Ring buffer overflow | Log error, increment counter, UI shows "Audio overrun" warning |

**Overflow handling:**

```rust
// In capture callback
if pushed < data.len() {
    overflow_counter.fetch_add(1, Ordering::Relaxed);
}

// In UI update loop
if overflow_counter.load(Ordering::Relaxed) > 0 {
    show_warning("Audio overrun detected. Transcript may be incomplete.");
}
```

**Design target:** With 30-second ring buffer and RTF < 0.5, overflow should never occur in normal operation. Overflow indicates either:
1. System under extreme load
2. Bug in processing thread
3. Unexpectedly slow Whisper inference

### 2.3 Transcription Strategy

**No full final pass.** Each VAD-bounded utterance is transcribed independently.

#### 2.3.1 Incremental Flow

```
Time: 00:00    00:10    00:20    00:30    00:40    00:50
       │        │        │        │        │        │
Audio: [speech] [silence] [speech] [silence] [speech] [STOP]
            ↓                 ↓                 ↓      ↓
       Utterance 1       Utterance 2      Utterance 3  Tail
            ↓                 ↓                 ↓      ↓
       Transcribe        Transcribe       Transcribe  Flush+Transcribe
            ↓                 ↓                 ↓      ↓
       Segment 1         Segment 2        Segment 3   Segment 4
            ↓                 ↓                 ↓      ↓
       ┌────┴────────────────┴────────────────┴───────┴──┐
       │                Final Transcript                  │
       └──────────────────────────────────────────────────┘
```

**On Stop:**
1. Flush any pending speech buffer (the "tail")
2. Transcribe the tail (bounded by `max_utterance_ms`, so < 25 seconds)
3. Append to accumulated segments
4. Final transcript = concatenation of all segments

**Stop latency guarantee:** With `max_utterance_ms = 25s`, the tail is at most 25 seconds of audio, transcribed in ~5 seconds worst case.

#### 2.3.2 Context Conditioning

Pass recent transcript as context for better accuracy:

```rust
fn transcribe_utterance(&self, audio: &[f32], context: &str) -> Result<String> {
    let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
    
    // Pass last ~50 words as context
    let context_words: Vec<_> = context.split_whitespace().rev().take(50).collect();
    let context_prompt = context_words.into_iter().rev().collect::<Vec<_>>().join(" ");
    
    params.set_initial_prompt(&context_prompt);
    params.set_n_threads(4);
    
    // Run inference...
}
```

### 2.4 Crash Recovery (OPTIONAL)

**POC Recommendation:** Skip crash recovery entirely. It adds complexity without proving the core loop.

**If implemented later:** Use OS file permissions (0600), no encryption. See v3.0 for details.

### 2.5 Apple Speech Provider (Deferred)

**POC Scope:** Stub implementation that returns "Not implemented."

---

## 3. Data Model

### 3.1 Segment Structure

```rust
use uuid::Uuid;
use chrono::{DateTime, Utc};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Segment {
    pub id: Uuid,
    pub start_ms: u64,
    pub end_ms: u64,
    pub text: String,
    
    // Future-proofing (optional for POC)
    pub speaker_id: Option<String>,
    pub avg_log_prob: Option<f32>,
    pub no_speech_prob: Option<f32>,
}
```

**Timestamp Semantics:**

| Field | Definition | Includes |
|-------|------------|----------|
| `start_ms` | Session-relative start time | Pre-roll audio (300ms before speech) |
| `end_ms` | Session-relative end time | Trailing silence until flush (up to 500ms) |
| `end_ms - start_ms` | Audio duration sent to Whisper | Full utterance including padding |

**Example timeline:**
```
Session time:    0ms      300ms        5000ms      5500ms
                  │         │            │           │
Audio:         [pre-roll][speech.......][silence...]│
                  │         │            │           │
Segment:        start_ms=0            end_ms=5500
```

**Why include padding in timestamps?** 
- Enables precise audio-transcript alignment for future features (playback sync, speaker diarization)
- Whisper receives this exact audio span, so timestamps match Whisper's internal timing

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionRecord {
    pub session_id: Uuid,
    pub provider: ProviderType,
    pub language: String,
    pub input_device_id: String,
    pub started_at: DateTime<Utc>,
    pub ended_at: Option<DateTime<Utc>>,
    pub segments: Vec<Segment>,
    
    // Stats
    pub total_duration_ms: u64,
    pub speech_duration_ms: u64,
    pub realtime_factor: Option<f32>,
}
```
```

### 3.2 Display vs Copy Formatting

**Critical distinction:** Display formatting and copy formatting are separate concerns.

```rust
/// For UI display (always paragraph style)
pub struct TranscriptDisplay {
    /// Finalized segments, each on its own line
    pub finalized_text: String,
    
    /// Current draft (if any), displayed in italic/gray
    pub draft_text: Option<String>,
}

impl TranscriptAssembler {
    /// Build display text (always paragraphs for readability)
    fn build_display_text(&self) -> String {
        self.segments
            .iter()
            .map(|s| s.text.as_str())
            .collect::<Vec<_>>()
            .join("\n\n")  // Always paragraph breaks for display
    }
}

/// For clipboard (respects OutputFormat setting)
pub struct TranscriptFormatter;

impl TranscriptFormatter {
    pub fn format(segments: &[Segment], format: OutputFormat) -> String {
        match format {
            OutputFormat::Paragraphs => {
                segments.iter()
                    .map(|s| s.text.as_str())
                    .collect::<Vec<_>>()
                    .join("\n\n")
            }
            OutputFormat::SingleParagraph => {
                segments.iter()
                    .map(|s| s.text.as_str())
                    .collect::<Vec<_>>()
                    .join(" ")
            }
        }
    }
}
```

---

## 4. State Machine

### 4.1 States

```rust
#[derive(Debug, Clone, PartialEq)]
pub enum SessionState {
    Idle,
    Preparing,      // Initializing audio, loading model
    Recording,      // Active capture and transcription
    Stopping,       // Flushing final utterance (bounded by max_utterance_ms)
    Completed,      // Ready for copy
    Error(SessionError),
}
```

### 4.2 Transitions

```
┌──────┐  Start   ┌───────────┐  Success  ┌───────────┐
│ Idle │─────────►│ Preparing │──────────►│ Recording │
└──────┘          └───────────┘           └─────┬─────┘
    ▲                   │                       │
    │                   │ Failure               │ Stop
    │                   ▼                       ▼
    │              ┌─────────┐           ┌──────────┐
    │              │  Error  │           │ Stopping │
    │              └────┬────┘           └────┬─────┘
    │                   │                     │
    │                   │ Reset               │ Success
    │                   │                     ▼
    │                   │             ┌───────────┐
    └───────────────────┴─────────────┤ Completed │───► Reset ───► Idle
                                      └───────────┘
```

**Stopping state duration:** Bounded by `max_utterance_ms` (25s) + transcription time (~5s) = ~30s worst case. Typical: < 5 seconds.

---

## 5. Functional Requirements

### 5.1 Core User Flow

1. User launches app
2. User selects input device (defaults to system default)
3. User clicks **Start**
4. App requests microphone permission if needed (macOS)
5. App shows: Recording indicator, elapsed time, live transcript
6. User clicks **Stop**
7. App shows brief "Finishing..." while processing tail
8. App shows final transcript with **Copy** button enabled
9. User clicks **Copy** → plain text to clipboard, success toast

### 5.2 Transcript Update Timing

**Unified promise:** Finalized transcript updates within 1–4 seconds of each utterance end.

- **Utterance end** = `silence_to_flush_ms` (500ms) of silence, OR `max_utterance_ms` (25s) reached
- **Update timing** = utterance transcription time (typically 1-3 seconds for 10-25s utterance)

**Draft text (optional enhancement):** If implemented, shows during active speech. Paused when `is_processing_behind` is true.

### 5.3 Settings

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settings {
    pub schema_version: u32,  // 1
    pub engine_mode: EngineMode,
    pub whisper_model: WhisperModel,
    pub language: String,  // "auto" | "en"
    pub input_device_id: Option<String>,
    pub output_format: OutputFormat,
    
    // VAD tuning
    pub vad_threshold: f32,           // 0.5 default
    pub vad_pre_roll_ms: u32,         // 300 default
    pub silence_to_flush_ms: u32,     // 500 default
    pub max_utterance_ms: u32,        // 25000 default
    
    // Model path (future: for "Import model..." feature)
    pub model_path: Option<String>,
}
```

### 5.4 Model Management

**POC:** Manual model placement in app data directory.

**Future:** "Import model..." button that stores explicit path in settings.

**Validation (loose sanity check):**

```rust
struct ModelValidation {
    min_size_mb: u64,
    max_size_mb: u64,
}

fn validate_model(path: &Path) -> Result<(), ModelError> {
    let size_mb = fs::metadata(path)?.len() / (1024 * 1024);
    
    // Loose checks that work across quantizations
    if size_mb < 30 {
        return Err(ModelError::FileTooSmall);
    }
    if size_mb > 4000 {
        return Err(ModelError::FileTooLarge);
    }
    
    // Warn (don't error) if size is unusual
    if size_mb < 50 || size_mb > 3000 {
        log::warn!("Model size {}MB is unusual", size_mb);
    }
    
    Ok(())
}
```

---

## 6. macOS Permissions

### 6.1 Required Info.plist Keys

```xml
<key>NSMicrophoneUsageDescription</key>
<string>This app needs microphone access to transcribe your speech.</string>
```

### 6.2 Entitlements

```xml
<key>com.apple.security.device.audio-input</key>
<true/>
```

---

## 7. IPC API

### 7.1 Commands

```typescript
declare function list_input_devices(): Promise<Device[]>;
declare function get_settings(): Promise<Settings>;
declare function set_settings(patch: Partial<Settings>): Promise<Settings>;
declare function start_session(): Promise<SessionInfo>;
declare function stop_session(): Promise<SessionSummary>;
declare function copy_transcript(format: 'paragraphs' | 'single_paragraph'): Promise<void>;
declare function check_model_status(): Promise<ModelStatus>;
```

### 7.2 Events

```typescript
interface SessionStatusEvent {
  state: 'idle' | 'preparing' | 'recording' | 'stopping' | 'completed' | 'error';
  provider: 'whisper' | 'apple' | null;
  elapsed_ms: number;
  is_processing_behind: boolean;
  error_message?: string;
}

interface TranscriptUpdateEvent {
  finalized_text: string;
  draft_text: string | null;
  segment_count: number;
}

listen('session_status', (event: SessionStatusEvent) => { ... });
listen('transcript_update', (event: TranscriptUpdateEvent) => { ... });
```

---

## 8. UI Specification

### 8.1 Layout

```
┌────────────────────────────────────────────────────────────┐
│  [Whisper]  ● Recording    00:05:23         [Processing...] │ ← Orange when behind
├────────────────────────────────────────────────────────────┤
│                                                            │
│  The quick brown fox jumps over the lazy dog.             │
│                                                            │
│  Pack my box with five dozen liquor jugs.                 │
│                                                            │
│  How vexingly quick daft zebras jump...                   │ ← Italic/gray (draft)
│                                                            │
├────────────────────────────────────────────────────────────┤
│  [● Stop]              [Copy]                    [⚙]      │
└────────────────────────────────────────────────────────────┘
```

### 8.2 Status Indicators

| State | Indicator | Notes |
|-------|-----------|-------|
| Recording | ● Green pulse | Normal operation |
| Recording (behind) | ● Orange + "Processing..." | Queue depth > 3 |
| Stopping | ◐ Yellow + "Finishing..." | Processing tail |

---

## 9. Testing Strategy

### 9.1 Test Commands

```makefile
# Quick check (CI, no model)
check:
	cd ui && pnpm lint && pnpm typecheck && pnpm test
	cd src-tauri && cargo fmt --check && cargo clippy -- -D warnings && cargo test
	cd cli && cargo fmt --check && cargo clippy -- -D warnings && cargo test

# Full test (local, requires model)
test-full:
	cd cli && cargo test -- --include-ignored
	cd src-tauri && cargo test -- --include-ignored
```

### 9.2 Key Unit Tests

```rust
// VAD gating
#[test] fn vad_preserves_timestamps_across_silence() { ... }
#[test] fn vad_enforces_max_utterance_length() { ... }
#[test] fn vad_applies_pre_roll_padding() { ... }

// Ring buffer
#[test] fn ring_buffer_handles_variable_input_fixed_output() { ... }

// Sample format conversion
#[test] fn converts_i16_to_f32() { ... }
#[test] fn converts_u8_to_f32() { ... }

// Formatter
#[test] fn format_paragraphs_uses_double_newlines() { ... }
#[test] fn format_single_paragraph_uses_spaces() { ... }
```

---

## 10. Performance Targets

| Metric | Target | Notes |
|--------|--------|-------|
| Real-Time Factor | < 0.5 | Primary metric (M1 + small model) |
| Update latency | 1-4 seconds | From utterance end to UI |
| Stop latency (typical) | < 10 seconds | Normal case: short tail + empty queue |
| Stop latency (worst) | < 75 seconds | Bounded by formula below |
| Memory (30 min) | < 1 GB | Including model |
| Ring buffer | 30 seconds | Never overflow in normal use |

**Stop Latency Formula:**

```
stop_wait = transcribe_time(tail_utterance) + Σ transcribe_time(queued_utterances)

Upper bound:
stop_wait_max = (queue_depth + 1) × max_utterance_ms × RTF
             = (5 + 1) × 25,000ms × 0.5
             = 75,000ms = 75 seconds (theoretical worst case)

Typical case (queue_depth=0, tail=5s):
stop_wait = 5,000ms × 0.5 = 2.5 seconds
```

**Why worst-case is acceptable:**
- Queue depth > 3 triggers visible "Processing..." warning
- User sees warning and knows system is behind
- In practice, silence gaps drain queue, so depth rarely exceeds 2-3
- 75s worst case requires continuous speech with zero pauses—rare in real usage

---

## 11. Risk Mitigations

| Risk | Impact | Mitigation |
|------|--------|------------|
| Whisper too slow | Poor UX | Start with `small`, allow `tiny`/`base` |
| VAD false negatives (speech → silence) | Missed speech | 300ms padding, configurable threshold |
| VAD false positives (silence → speech) | Garbage/short segments | `min_speech_ms` filter |
| Long continuous speech | Whisper limit exceeded | `max_utterance_ms = 25s` hard limit |
| Ring buffer overflow | Lost audio | 30s buffer, backpressure warnings |
| Unsupported audio format | Crash | Handle f32/i16/u8 explicitly |
| VAD ONNX runtime packaging | Build complexity | Verify early; `voice_activity_detector` bundles model |

**M0 Verification Required:** Before committing to `voice_activity_detector`, verify:
1. ONNX model is bundled (not downloaded at runtime)
2. Builds cleanly on macOS (ARM + Intel)
3. No unexpected runtime dependencies
4. Binary size impact is acceptable (~10-20MB expected)

---

## 12. Milestones

### M0: Headless CLI (Week 1) — ✅ COMPLETE

**Build the pipeline before touching UI.**

```bash
./transcribe-cli --model models/ggml-small.bin --device default
# Captures audio, transcribes, prints to stdout
```

Deliverables:
- [x] cpal capture with format handling (f32/i16/u8)
- [x] Resampling to 16kHz
- [x] Ring buffer (pre-resample, raw device rate)
- [x] VAD gating with correct timestamp tracking
- [x] Max utterance enforcement
- [x] whisper-rs inference
- [x] stdout output
- [x] Unit tests for all components

### M1: Tauri Skeleton (Week 2) — ✅ COMPLETE

- [x] Tauri scaffold
- [x] State machine with tests
- [x] Settings persistence
- [x] Basic UI layout

### M2: Integration (Week 2-3) — ✅ COMPLETE

- [x] Integrate CLI pipeline
- [x] Event emission
- [x] Live UI updates
- [x] Device selection

### M3: Polish (Week 3-4) — ✅ COMPLETE

- [x] macOS permissions
- [x] Copy with formatting
- [x] Error handling
- [x] Performance monitoring
- [x] Documentation

### M4: Optional Enhancements — ✅ COMPLETE (Extended)

- [x] Speaker diarization
- [x] Speech enhancement (GTCRN)
- [x] Emotion detection (wav2small)
- [x] SOAP note generation (Ollama)
- [x] Audio events in SOAP context
- [x] Biomarker analysis (vitality, stability, cough detection)
- [x] Medplum EMR integration
- [x] Encounter history window
- [x] Audio preprocessing (DC removal, high-pass, AGC)
- [x] Conversation dynamics

### M5: Windows (Future)

- [ ] Platform-specific fixes
- [ ] Installer

---

## 13. Glossary

| Term | Definition |
|------|------------|
| VAD | Voice Activity Detection |
| Utterance | Continuous speech bounded by silence or max length |
| RTF | Real-Time Factor (transcription time / audio time) |
| Pre-roll | Audio before detected speech start (for soft consonants) |
| Silence-to-flush | How long to wait after speech ends before processing |
| Max utterance | Hard limit preventing Whisper's 30s overflow |

---

## Changelog

### v3.3 (Final)
- **Fixed:** VAD chunk size mismatch — added 16kHz staging buffer after resampler
- **Fixed:** Channel handling — downmix stereo to mono in callback
- **Fixed:** Timestamp off-by-one — `chunk_start_samples()` accounts for audio clock being at END
- **Fixed:** Forced-flush timestamps — now subtracts pre-roll (consistent with normal start)
- **Fixed:** Stop latency wording — "typical <10s, worst-case bounded by formula"
- **Fixed:** M0 milestone — ring buffer correctly described as pre-resample
- **Added:** Device config preference (prefer mono input when available)

### v3.2
- Resampler moved to processing thread (realtime-safe callback)
- Ring buffer now pre-resample, stores raw device-rate audio
- Audio clock uses sample counter (not wall-clock)
- Pre-roll duplication bug fixed
- Overflow policy made honest

### v3.1
- Fixed audio dropping contradiction
- Added sample format handling (f32/i16/u8)
- Separated display vs copy formatting

### v3.0
- VAD mandatory, utterance-based transcription, M0 CLI-first approach

### v2.0
- Added VAD requirement, near-real-time terminology

### v1.0
- Initial specification

---

*Specification Version: 3.3 (Final)*  
*Last Updated: December 2024*  
*Status: ✅ Approved for Development — Stop writing specs, start coding*  
*Next Step: `cargo new transcribe-cli`*
