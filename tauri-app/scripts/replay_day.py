#!/usr/bin/env python3
"""
Re-transcribe a day's continuous-mode audio and replay encounter detection +
SOAP generation under one or more LLM configurations.

Use when:
  - The STT server had a bug on a specific day and you want to see how the
    day would have looked with clean transcripts.
  - You want to A/B two LLM configurations (e.g. `fast-model` vs `soap-alt`)
    on identical transcripts to compare detection accuracy and SOAP quality.

What it does:
  - Phase 1 (transcribe): slices every `continuous_YYYYMMDD_*.wav` in
    `~/.transcriptionapp/recordings/` into 5-minute chunks, submits each via
    the STT Router's `/v1/audio/transcribe/medical-batch` endpoint (4 parallel),
    and caches the results on disk.
  - Phase 2 (replay): walks the cached transcripts through a faithful
    reimplementation of the continuous-mode detector loop — periodic LLM
    detection checks, confidence gate, per-encounter SOAP generation. The
    production prompts (detection + SOAP) are copied verbatim from
    `encounter_detection.rs` and `llm_client.rs`.
  - Phase 3 (compare): prints a side-by-side summary + the original day_log
    splits for that date.

Usage:
  # 1. Re-transcribe all audio for a date (one-time, cached).
  python3 scripts/replay_day.py transcribe 2026-04-10

  # 2. Replay with default config (fast-model + soap-model-fast)
  python3 scripts/replay_day.py replay 2026-04-10 default

  # 3. Replay with soap-alt for both detection and SOAP
  python3 scripts/replay_day.py replay 2026-04-10 soap_alt

  # 4. Side-by-side comparison + ground-truth day_log
  python3 scripts/replay_day.py compare 2026-04-10

Output layout: /tmp/replay_<YYYY-MM-DD>/
    audio_chunks/{run_id}/chunk_####.wav
    transcripts/{run_id}/chunks.jsonl
    configs/{config_name}/
        summary.json
        detection_log_{run_id}.jsonl
        encounters/enc_{NNN}/
            transcript.txt
            soap_raw.txt
            soap_note.json
            meta.json

Requirements: python3 (stdlib only), ffmpeg, curl. LLM and STT routers must
be reachable at the URLs below. Auth is read from ~/.transcriptionapp/config.json.
"""

import json
import re
import subprocess
import sys
import time
from concurrent.futures import ThreadPoolExecutor, as_completed
from pathlib import Path

import urllib.request
import urllib.error

# ─── Configuration ─────────────────────────────────────────────────────────────

STT_BASE = "http://100.119.83.76:8001"
STT_ALIAS = "medical-batch"
LLM_BASE = "http://100.119.83.76:8080"

CHUNK_SECS = 300          # 5-minute STT batch slices.
                          # <60s triggers Qwen silence-hallucinations because
                          # language auto-detect needs enough audio context.
STT_CONCURRENCY = 4       # parallel STT requests
DETECTION_INTERVAL = 120  # matches production's `encounter_check_interval_secs`
MIN_DETECTION_WORDS = 100 # matches `MIN_SPLIT_WORD_FLOOR` and detection word-gate
ABSOLUTE_WORD_CAP = 25_000  # matches encounter_detection.rs::ABSOLUTE_WORD_CAP
LLM_TIMEOUT = 90          # matches continuous_mode.rs LLM call timeout

RECORDINGS_DIR = Path.home() / ".transcriptionapp" / "recordings"
ARCHIVE_DIR = Path.home() / ".transcriptionapp" / "archive"
CONFIG_JSON = Path.home() / ".transcriptionapp" / "config.json"

CONFIGS = {
    "default": {
        "detection_model": "fast-model",
        "soap_model": "soap-model-fast",
    },
    "soap_alt": {
        "detection_model": "soap-alt",
        "soap_model": "soap-alt",
    },
    "soap_alt_2": {
        "detection_model": "soap-alt-2",
        "soap_model": "soap-alt-2",
    },
}

# ─── Production prompts (copied verbatim from Rust source) ──────────────────
# Keep in sync with:
#   - encounter_detection.rs::build_encounter_detection_prompt
#   - llm_client.rs::build_simple_soap_prompt

DETECTION_SYSTEM_PROMPT = """You MUST respond in English with ONLY a JSON object. No other text, no explanations, no markdown.

You are analyzing a continuous transcript from a medical office where the microphone records all day.

Your task: determine if there is a TRANSITION POINT where one patient encounter ends and another begins, or where a patient encounter has clearly concluded.

A completed encounter typically includes a clinical discussion and a concluding plan (e.g., medication changes, follow-up timing, referrals, or instructions). A short transcript that contains only vitals, greetings, or brief exchanges — with no clinical discussion or plan — is likely a pre-visit assessment still in progress, not a completed encounter. A nurse or medical assistant may see the patient before the doctor arrives to take vitals or ask initial questions — this is part of the same encounter, not a separate visit.

Each segment includes elapsed time (MM:SS) from the start of the recording. Large gaps between timestamps may indicate silence, examination, or the room being empty between patients.

If you find a transition point or completed encounter, return:
{"complete": true, "end_segment_index": <last segment index of the CONCLUDED encounter>, "confidence": <0.0-1.0>}

If the current discussion is still one ongoing encounter with no transition, return:
{"complete": false, "confidence": <0.0-1.0>}

Respond with ONLY the JSON object."""

SOAP_SYSTEM_PROMPT = """You are a medical scribe that outputs ONLY valid JSON. Extract clinical information from transcripts into SOAP notes.

The transcript is from speech-to-text and may contain errors. Interpret medical terms correctly:
- "human blade 1c" or "h b a 1 c" → HbA1c (hemoglobin A1c)
- "ekg" or "e k g" → EKG/ECG
- Homophones and phonetic errors are common - use clinical context

RESPOND WITH ONLY THIS JSON STRUCTURE - NO OTHER TEXT:
{"subjective":["item"],"objective":["item"],"assessment":["item"],"plan":["item"]}

ORGANIZATION: Create a single unified SOAP note covering all problems together in each section. Do NOT prefix items with problem labels.

SECTION DEFINITIONS:
- SUBJECTIVE: What the patient reports — symptoms, complaints, history of present illness, past medical/surgical history, medication history, social history, family history, review of systems, and any information prefaced by "patient reports/states/denies/describes". Also include historical test results the patient or physician recounts from previous visits (e.g. "previous EKG showed...", "labs from September...").
- OBJECTIVE: ONLY findings from TODAY'S encounter — vital signs measured today, physical examination findings observed by the clinician, point-of-care test results obtained today, and imaging/lab results reviewed for the first time today. If the physician did not perform an exam or obtain new results, use an empty array []. Do NOT put patient-reported information here. Do NOT put historical results, prior imaging, or previous lab values here — those go in Subjective.
- ASSESSMENT: Clinical impressions, diagnoses, differential diagnoses, and the clinician's interpretation of the findings.
- PLAN: Treatments ordered, prescriptions, referrals, follow-up instructions, procedures performed, patient education provided, and next steps — ONLY what the doctor actually stated. Do NOT add recommendations not mentioned in the transcript.

Rules:
- Your entire response must be valid JSON - nothing else
- Use simple string arrays, no nested objects
- Do NOT use newlines inside JSON strings - keep each array item as a single line
- Use empty arrays [] for sections with no information
- Use correct medical terminology
- Do NOT use any markdown formatting (no **, no __, no #, no backticks) - output plain text only
- Do NOT include specific patient names or healthcare provider names - use "patient" or "the physician/provider" instead
- Do NOT hallucinate or embellish - only include what was explicitly stated
- DETAIL LEVEL: 4/10 - Use STANDARD clinical detail. Include key findings and relevant history."""

# ─── Date parsing + audio file discovery ─────────────────────────────────────

DATE_RE = re.compile(r"^(\d{4})[-/](\d{2})[-/](\d{2})$")

def parse_date(s: str) -> tuple[str, str, str]:
    m = DATE_RE.match(s)
    if not m:
        raise ValueError(f"Invalid date '{s}' — expected YYYY-MM-DD or YYYY/MM/DD")
    return m.group(1), m.group(2), m.group(3)

def date_compact(date: str) -> str:
    """'2026-04-10' → '20260410' (matches WAV file naming)."""
    y, m, d = parse_date(date)
    return f"{y}{m}{d}"

def work_dir_for(date: str) -> Path:
    y, m, d = parse_date(date)
    return Path(f"/tmp/replay_{y}-{m}-{d}")

def archive_day_dir(date: str) -> Path:
    y, m, d = parse_date(date)
    return ARCHIVE_DIR / y / m / d

def discover_audio_files(date: str) -> list[tuple[Path, str, str]]:
    """Return [(wav_path, run_id, start_hhmmss)] for every continuous-mode WAV
    matching the given date. Sorted chronologically by filename timestamp.

    Filename format: `continuous_YYYYMMDD_HHMMSS.wav`
    """
    compact = date_compact(date)
    pattern = f"continuous_{compact}_*.wav"
    files = sorted(RECORDINGS_DIR.glob(pattern))
    results = []
    for i, wav in enumerate(files, 1):
        m = re.match(r"continuous_\d{8}_(\d{2})(\d{2})(\d{2})\.wav$", wav.name)
        if not m:
            continue
        hh, mm, ss = m.groups()
        run_id = f"run{i}_{hh}{mm}{ss}"
        results.append((wav, run_id, f"{hh}:{mm}:{ss}"))
    return results

# ─── Auth ────────────────────────────────────────────────────────────────────

def load_auth() -> tuple[str, str]:
    """Read LLM auth credentials from ~/.transcriptionapp/config.json.
    Falls back to clinic defaults if the file isn't present."""
    defaults = ("ai-scribe-secret-key", "ai-scribe")
    if not CONFIG_JSON.exists():
        return defaults
    try:
        cfg = json.loads(CONFIG_JSON.read_text())
        return (
            cfg.get("llm_api_key", defaults[0]),
            cfg.get("llm_client_id", defaults[1]),
        )
    except (json.JSONDecodeError, OSError):
        return defaults

LLM_API_KEY, LLM_CLIENT_ID = load_auth()

# ─── Helpers ─────────────────────────────────────────────────────────────────

def fmt_mmss(secs: float) -> str:
    secs = int(secs)
    h, rem = divmod(secs, 3600)
    m, s = divmod(rem, 60)
    if h:
        return f"{h}:{m:02d}:{s:02d}"
    return f"{m:02d}:{s:02d}"

def http_post_json(url: str, body: dict, headers: dict, timeout: int = 60) -> dict:
    data = json.dumps(body).encode("utf-8")
    req = urllib.request.Request(url, data=data, method="POST")
    for k, v in headers.items():
        req.add_header(k, v)
    req.add_header("Content-Type", "application/json")
    try:
        with urllib.request.urlopen(req, timeout=timeout) as resp:
            return json.loads(resp.read().decode("utf-8"))
    except urllib.error.HTTPError as e:
        body_text = e.read().decode("utf-8", errors="replace")
        raise RuntimeError(f"HTTP {e.code} from {url}: {body_text}")

def http_post_multipart(url: str, file_path: Path, fields: dict, timeout: int = 120) -> dict:
    """curl for multipart — Python's urllib doesn't do multipart cleanly."""
    cmd = ["curl", "-s", "--max-time", str(timeout), "-X", "POST", url, "-F", f"file=@{file_path}"]
    for k, v in fields.items():
        cmd += ["-F", f"{k}={v}"]
    out = subprocess.run(cmd, capture_output=True, text=True)
    if out.returncode != 0:
        raise RuntimeError(f"curl failed: {out.stderr}")
    return json.loads(out.stdout)

def llm_chat(model: str, system: str, user: str, max_tokens: int = 2000) -> str:
    body = {
        "model": model,
        "messages": [
            {"role": "system", "content": system},
            {"role": "user", "content": user},
        ],
        "max_tokens": max_tokens,
        "temperature": 0.3,
    }
    headers = {
        "Authorization": f"Bearer {LLM_API_KEY}",
        "X-Client-ID": LLM_CLIENT_ID,
        "X-Clinic-Task": "encounter_replay",
    }
    resp = http_post_json(f"{LLM_BASE}/v1/chat/completions", body, headers, timeout=LLM_TIMEOUT)
    return resp["choices"][0]["message"]["content"]

def parse_json_object(text: str) -> dict | None:
    """Extract the first balanced JSON object from an LLM response.
    Tolerates <think> tags, markdown fences, trailing prose."""
    while "<think>" in text and "</think>" in text:
        s = text.index("<think>")
        e = text.index("</think>") + len("</think>")
        text = text[:s] + text[e:]
    text = text.strip()
    if text.startswith("```"):
        nl = text.find("\n")
        if nl != -1:
            text = text[nl + 1:]
        if text.endswith("```"):
            text = text[:-3]
    text = text.strip()
    if "{" not in text:
        return None
    start = text.index("{")
    depth = 0
    end = -1
    in_str = False
    esc = False
    for i, ch in enumerate(text[start:], start):
        if esc:
            esc = False
            continue
        if ch == "\\":
            esc = True
            continue
        if ch == '"':
            in_str = not in_str
            continue
        if in_str:
            continue
        if ch == "{":
            depth += 1
        elif ch == "}":
            depth -= 1
            if depth == 0:
                end = i + 1
                break
    if end == -1:
        return None
    try:
        return json.loads(text[start:end])
    except json.JSONDecodeError:
        return None

# ─── Phase 1: STT re-transcription ──────────────────────────────────────────

def chunk_audio_file(wav_path: Path, run_id: str, work_dir: Path) -> list[Path]:
    out_dir = work_dir / "audio_chunks" / run_id
    out_dir.mkdir(parents=True, exist_ok=True)
    existing = sorted(out_dir.glob("chunk_*.wav"))
    if existing:
        print(f"  [{run_id}] using cached {len(existing)} chunks")
        return existing

    probe = subprocess.run(
        ["ffprobe", "-v", "error", "-show_entries", "format=duration",
         "-of", "default=noprint_wrappers=1:nokey=1", str(wav_path)],
        capture_output=True, text=True, check=True,
    )
    duration = float(probe.stdout.strip())
    n_chunks = int(duration // CHUNK_SECS) + (1 if duration % CHUNK_SECS > 0 else 0)
    print(f"  [{run_id}] {wav_path.name}: {duration:.0f}s → {n_chunks} chunks")

    for i in range(n_chunks):
        start = i * CHUNK_SECS
        out_path = out_dir / f"chunk_{i:04d}.wav"
        subprocess.run(
            ["ffmpeg", "-hide_banner", "-loglevel", "error", "-y",
             "-ss", str(start), "-t", str(CHUNK_SECS),
             "-i", str(wav_path), "-ar", "16000", "-ac", "1", "-c:a", "pcm_s16le",
             str(out_path)],
            check=True,
        )
    return sorted(out_dir.glob("chunk_*.wav"))

def transcribe_chunk(chunk_path: Path) -> dict:
    """POST one chunk to the STT batch endpoint.

    Language is intentionally NOT specified — Qwen's auto-detect avoids the
    silence-hallucination artifacts ("I'm not sure.", "make sure your car has
    a valid registration.") that appear when an explicit language directive is
    combined with sub-minute audio chunks.
    """
    url = f"{STT_BASE}/v1/audio/transcribe/{STT_ALIAS}"
    return http_post_multipart(url, chunk_path, {
        "response_format": "verbose_json",
        "postprocess": "true",
    })

def run_transcription(date: str):
    work_dir = work_dir_for(date)
    work_dir.mkdir(parents=True, exist_ok=True)

    print("=" * 80)
    print(f"PHASE 1: STT re-transcription [date={date}]")
    print("=" * 80)

    audio_files = discover_audio_files(date)
    if not audio_files:
        print(f"No continuous-mode WAVs found for {date} in {RECORDINGS_DIR}/")
        print(f"(expected pattern: continuous_{date_compact(date)}_*.wav)")
        sys.exit(1)

    print(f"Found {len(audio_files)} continuous-mode run(s):")
    for wav, run_id, start_str in audio_files:
        size_mb = wav.stat().st_size / (1024 * 1024)
        print(f"  {run_id}  {start_str}  {size_mb:.0f}MB  {wav.name}")
    print()

    for wav, run_id, _ in audio_files:
        out_jsonl = work_dir / "transcripts" / run_id / "chunks.jsonl"
        if out_jsonl.exists():
            n_lines = sum(1 for _ in out_jsonl.open())
            print(f"  [{run_id}] cached: {n_lines} chunks")
            continue

        out_jsonl.parent.mkdir(parents=True, exist_ok=True)
        chunks = chunk_audio_file(wav, run_id, work_dir)
        print(f"  [{run_id}] transcribing {len(chunks)} chunks via STT batch ({STT_CONCURRENCY} parallel)...")

        results = [None] * len(chunks)
        with ThreadPoolExecutor(max_workers=STT_CONCURRENCY) as ex:
            futures = {ex.submit(transcribe_chunk, c): i for i, c in enumerate(chunks)}
            done = 0
            for fut in as_completed(futures):
                i = futures[fut]
                try:
                    results[i] = fut.result()
                except Exception as e:
                    print(f"    chunk {i} error: {e}")
                    results[i] = {"text": "", "error": str(e)}
                done += 1
                if done % 20 == 0 or done == len(chunks):
                    print(f"    {done}/{len(chunks)}")

        with out_jsonl.open("w") as f:
            for i, r in enumerate(results):
                row = {
                    "chunk_index": i,
                    "start_secs": i * CHUNK_SECS,
                    "end_secs": (i + 1) * CHUNK_SECS,
                    "text": (r or {}).get("text", "").strip(),
                    "error": (r or {}).get("error"),
                }
                f.write(json.dumps(row) + "\n")
        print(f"  [{run_id}] wrote {out_jsonl}")

    print()
    print("Phase 1 complete.")

# ─── Phase 2: Detection + SOAP replay ────────────────────────────────────────

# Known silence-hallucination fillers emitted by Qwen on empty/quiet chunks.
# Drop them before handing segments to the LLM.
SILENCE_FILLERS = {"", "i'm not sure.", "i'm not sure", "thank you.", "thank you", "thanks.", "thanks"}

def load_segments(work_dir: Path, run_id: str) -> list[dict]:
    path = work_dir / "transcripts" / run_id / "chunks.jsonl"
    segments = []
    idx = 0
    for line in path.open():
        row = json.loads(line)
        text = row["text"]
        if text.strip().lower() in SILENCE_FILLERS:
            continue
        segments.append({
            "index": idx,
            "start_secs": row["start_secs"],
            "end_secs": row["end_secs"],
            "text": text,
        })
        idx += 1
    return segments

def format_segments_for_detection(segments: list[dict]) -> str:
    """Match production's `[index] (MM:SS) (Speaker): text` format.
    Batch STT doesn't return speaker labels, so we use 'Unknown'."""
    if not segments:
        return ""
    first_start = segments[0]["start_secs"]
    lines = []
    for s in segments:
        elapsed = s["start_secs"] - first_start
        lines.append(f"[{s['index']}] ({fmt_mmss(elapsed)}) (Unknown): {s['text']}")
    return "\n".join(lines)

def replay_run(run_id: str, config_name: str, config: dict,
               encounter_counter: list[int], work_dir: Path) -> list[dict]:
    segments = load_segments(work_dir, run_id)
    if not segments:
        print(f"  [{run_id}] no segments")
        return []
    print(f"  [{run_id}] {len(segments)} segments, {sum(len(s['text'].split()) for s in segments)} words")

    for i, s in enumerate(segments):
        s["index"] = i

    encounters = []
    buffer_segments = []
    buffer_first_secs = None
    last_check_at = -DETECTION_INTERVAL
    detection_log = []

    out_dir = work_dir / "configs" / config_name
    out_dir.mkdir(parents=True, exist_ok=True)
    detection_log_path = out_dir / f"detection_log_{run_id}.jsonl"

    def emit_encounter(end_idx_in_buffer: int, trigger: str):
        nonlocal buffer_segments, buffer_first_secs
        if end_idx_in_buffer >= len(buffer_segments):
            end_idx_in_buffer = len(buffer_segments) - 1
        encounter_segs = buffer_segments[: end_idx_in_buffer + 1]
        rest = buffer_segments[end_idx_in_buffer + 1:]
        if not encounter_segs:
            return

        enc_num = encounter_counter[0]
        encounter_counter[0] += 1
        word_count = sum(len(s["text"].split()) for s in encounter_segs)
        text = " ".join(s["text"] for s in encounter_segs)
        formatted = format_segments_for_detection(encounter_segs)
        first_secs = encounter_segs[0]["start_secs"]
        last_secs = encounter_segs[-1]["end_secs"]
        print(f"    [{config_name}] enc #{enc_num} ({trigger}): {word_count}w, {fmt_mmss(last_secs - first_secs)}")

        enc_dir = out_dir / "encounters" / f"enc_{enc_num:03d}"
        enc_dir.mkdir(parents=True, exist_ok=True)
        (enc_dir / "transcript.txt").write_text(formatted)
        (enc_dir / "meta.json").write_text(json.dumps({
            "run_id": run_id,
            "encounter_number": enc_num,
            "trigger": trigger,
            "word_count": word_count,
            "first_secs": first_secs,
            "last_secs": last_secs,
            "duration_secs": last_secs - first_secs,
        }, indent=2))

        if word_count >= MIN_DETECTION_WORDS:
            try:
                t0 = time.time()
                soap_raw = llm_chat(config["soap_model"], SOAP_SYSTEM_PROMPT, text, max_tokens=4000)
                soap_latency = time.time() - t0
                (enc_dir / "soap_raw.txt").write_text(soap_raw)
                parsed = parse_json_object(soap_raw)
                (enc_dir / "soap_note.json").write_text(json.dumps(
                    parsed or {"parse_error": True}, indent=2
                ))
                print(f"      SOAP: {soap_latency:.1f}s, parsed={parsed is not None}")
            except Exception as e:
                print(f"      SOAP error: {e}")
                (enc_dir / "soap_error.txt").write_text(str(e))

        encounters.append({
            "encounter_number": enc_num,
            "run_id": run_id,
            "trigger": trigger,
            "word_count": word_count,
            "first_secs": first_secs,
            "last_secs": last_secs,
        })

        buffer_segments[:] = rest
        for i, s in enumerate(buffer_segments):
            s["index"] = i
        buffer_first_secs = buffer_segments[0]["start_secs"] if buffer_segments else None

    for seg in segments:
        if buffer_first_secs is None:
            buffer_first_secs = seg["start_secs"]
        seg = dict(seg)
        seg["index"] = len(buffer_segments)
        buffer_segments.append(seg)

        elapsed_in_buffer = seg["end_secs"] - buffer_first_secs
        word_count = sum(len(s["text"].split()) for s in buffer_segments)

        if word_count >= ABSOLUTE_WORD_CAP:
            print(f"    [{config_name}] absolute word cap at {word_count}w → force split")
            emit_encounter(len(buffer_segments) - 1, "absolute_word_cap")
            last_check_at = elapsed_in_buffer
            continue

        if elapsed_in_buffer - last_check_at >= DETECTION_INTERVAL and word_count >= MIN_DETECTION_WORDS:
            last_check_at = elapsed_in_buffer
            formatted = format_segments_for_detection(buffer_segments)
            try:
                t0 = time.time()
                resp = llm_chat(
                    config["detection_model"],
                    DETECTION_SYSTEM_PROMPT,
                    f"Transcript (segments numbered with speaker labels):\n{formatted}",
                    max_tokens=200,
                )
                parsed = parse_json_object(resp)
                detection_log.append({
                    "buffer_age_secs": elapsed_in_buffer,
                    "word_count": word_count,
                    "model": config["detection_model"],
                    "latency_secs": time.time() - t0,
                    "raw_response": resp,
                    "parsed": parsed,
                })
                if parsed and parsed.get("complete"):
                    confidence = parsed.get("confidence", 0.0)
                    end_idx = parsed.get("end_segment_index")
                    # Match production's dynamic confidence gate.
                    threshold = 0.85 if elapsed_in_buffer < 1200 else 0.7
                    if (
                        confidence >= threshold
                        and isinstance(end_idx, int)
                        and 0 <= end_idx < len(buffer_segments)
                    ):
                        split_wc = sum(
                            len(s["text"].split())
                            for s in buffer_segments[: end_idx + 1]
                        )
                        if split_wc >= MIN_DETECTION_WORDS:
                            emit_encounter(end_idx, "llm")
            except Exception as e:
                detection_log.append({
                    "buffer_age_secs": elapsed_in_buffer,
                    "word_count": word_count,
                    "model": config["detection_model"],
                    "error": str(e),
                })
                print(f"    detection error: {e}")

    # Flush remaining buffer at end of run.
    if buffer_segments and sum(len(s["text"].split()) for s in buffer_segments) >= MIN_DETECTION_WORDS:
        emit_encounter(len(buffer_segments) - 1, "flush_on_end")

    with detection_log_path.open("w") as f:
        for row in detection_log:
            f.write(json.dumps(row) + "\n")

    return encounters

def run_replay(date: str, config_name: str):
    if config_name not in CONFIGS:
        print(f"Unknown config '{config_name}'. Known: {', '.join(CONFIGS)}")
        sys.exit(1)

    work_dir = work_dir_for(date)
    config = CONFIGS[config_name]

    print("=" * 80)
    print(f"PHASE 2: detection + SOAP replay [date={date}, config={config_name}]")
    print("=" * 80)
    print(f"  detection_model={config['detection_model']}")
    print(f"  soap_model={config['soap_model']}")
    print()

    audio_files = discover_audio_files(date)
    if not audio_files:
        print(f"No audio files for {date}")
        sys.exit(1)

    encounter_counter = [1]
    all_encounters = []
    for _, run_id, _ in audio_files:
        if not (work_dir / "transcripts" / run_id / "chunks.jsonl").exists():
            print(f"  [{run_id}] SKIPPED — no transcripts (run 'transcribe' first)")
            continue
        encounters = replay_run(run_id, config_name, config, encounter_counter, work_dir)
        all_encounters.extend(encounters)

    summary_path = work_dir / "configs" / config_name / "summary.json"
    summary_path.write_text(json.dumps({
        "date": date,
        "config": config,
        "n_encounters": len(all_encounters),
        "encounters": all_encounters,
    }, indent=2))
    print()
    print(f"Pass [{config_name}] complete: {len(all_encounters)} encounters")
    print(f"Summary: {summary_path}")

# ─── Phase 3: Comparison report ──────────────────────────────────────────────

def run_compare(date: str):
    work_dir = work_dir_for(date)
    print("=" * 80)
    print(f"COMPARISON [date={date}]")
    print("=" * 80)

    for cn in CONFIGS:
        summary_path = work_dir / "configs" / cn / "summary.json"
        if not summary_path.exists():
            continue
        s = json.loads(summary_path.read_text())
        print(f"\n[{cn}] {s['n_encounters']} encounters detected")
        for e in s["encounters"]:
            print(f"  enc#{e['encounter_number']:03d}  {e['run_id']:<18}  "
                  f"{fmt_mmss(e['first_secs'])}-{fmt_mmss(e['last_secs'])}  "
                  f"{e['word_count']:>5}w  trigger={e['trigger']}")

    day_log = archive_day_dir(date) / "day_log.jsonl"
    if day_log.exists():
        print(f"\n[ORIGINAL {date} day_log] {day_log}")
        splits = 0
        merges = 0
        for line in day_log.open():
            try:
                e = json.loads(line)
            except json.JSONDecodeError:
                continue
            if e.get("event") == "encounter_split":
                splits += 1
                print(f"  {e.get('ts','?')[:19]}  trigger={e.get('trigger'):<25}  wc={e.get('word_count')}")
            elif e.get("event") == "encounter_merged":
                merges += 1
        print(f"  Total: {splits} splits, {merges} merge-backs → {splits - merges} net encounters")
    else:
        print(f"\n(no production day_log at {day_log})")

# ─── Main ────────────────────────────────────────────────────────────────────

def main():
    if len(sys.argv) < 2 or sys.argv[1] in ("-h", "--help", "help"):
        print(__doc__)
        sys.exit(0 if len(sys.argv) >= 2 else 1)

    cmd = sys.argv[1]

    if cmd == "transcribe":
        if len(sys.argv) < 3:
            print("usage: replay_day.py transcribe <YYYY-MM-DD>")
            sys.exit(1)
        run_transcription(sys.argv[2])
    elif cmd == "replay":
        if len(sys.argv) < 4:
            print("usage: replay_day.py replay <YYYY-MM-DD> <config_name>")
            print(f"known configs: {', '.join(CONFIGS)}")
            sys.exit(1)
        run_replay(sys.argv[2], sys.argv[3])
    elif cmd == "compare":
        if len(sys.argv) < 3:
            print("usage: replay_day.py compare <YYYY-MM-DD>")
            sys.exit(1)
        run_compare(sys.argv[2])
    else:
        print(f"unknown command: {cmd}")
        print(__doc__)
        sys.exit(1)

if __name__ == "__main__":
    main()
