# Whisper Server: Missing FFmpeg Dependency

## Issue Summary

The Whisper transcription server at `http://10.241.15.154:8001` is returning 500 errors when processing audio transcription requests due to a missing `ffmpeg` dependency.

## Error Details

```
500 Internal Server Error
{"detail":"[Errno 2] No such file or directory: 'ffmpeg'"}
```

## Root Cause

The faster-whisper/speaches server requires `ffmpeg` to decode incoming audio files (WAV, MP3, etc.) before transcription. The server is running but `ffmpeg` is not installed or not in the system PATH.

## Current Server Status

- **URL**: `http://10.241.15.154:8001`
- **API**: OpenAI-compatible (`/v1/audio/transcriptions`, `/v1/models`)
- **Model Available**: `large-v3-turbo`
- **Health Check**: `/v1/models` endpoint responds correctly
- **Transcription**: Fails with ffmpeg error

## Solution

### Option 1: Docker (Recommended)

If running via Docker, use an image that includes ffmpeg:

```bash
# Stop current container
docker stop <container_id>

# Run with ffmpeg-included image
# For GPU (CUDA):
docker run -d \
  --name whisper-server \
  --gpus all \
  -p 8001:8000 \
  ghcr.io/speaches-ai/speaches:latest-cuda

# For CPU-only:
docker run -d \
  --name whisper-server \
  -p 8001:8000 \
  ghcr.io/speaches-ai/speaches:latest-cpu
```

If using a custom Dockerfile, add ffmpeg:

```dockerfile
FROM python:3.11-slim

# Install ffmpeg
RUN apt-get update && apt-get install -y ffmpeg && rm -rf /var/lib/apt/lists/*

# ... rest of your Dockerfile
```

### Option 2: Local Installation

If running directly on the host machine:

**macOS:**
```bash
brew install ffmpeg
```

**Ubuntu/Debian:**
```bash
sudo apt-get update
sudo apt-get install -y ffmpeg
```

**RHEL/CentOS/Rocky:**
```bash
sudo dnf install -y ffmpeg
# Or if using EPEL:
sudo dnf install -y epel-release
sudo dnf install -y ffmpeg
```

**Verify installation:**
```bash
ffmpeg -version
which ffmpeg
```

### Option 3: Python Environment

If running in a Python virtual environment, ensure ffmpeg is accessible:

```bash
# The ffmpeg binary must be in PATH when the server starts
export PATH="/usr/local/bin:$PATH"  # or wherever ffmpeg is installed

# Then start the server
python -m speaches.main  # or however the server is started
```

## Verification Steps

After installing ffmpeg, verify the fix:

1. **Check ffmpeg is available:**
   ```bash
   ffmpeg -version
   ```

2. **Restart the Whisper server**

3. **Test transcription endpoint:**
   ```bash
   # Create a test audio file (or use existing)
   curl -X POST "http://10.241.15.154:8001/v1/audio/transcriptions" \
     -H "Content-Type: multipart/form-data" \
     -F "file=@test_audio.wav" \
     -F "model=large-v3-turbo" \
     -F "language=en"
   ```

4. **Expected response:**
   ```json
   {"text": "transcribed text here..."}
   ```

## Client Configuration Reference

The transcription app is configured to use:

| Setting | Value |
|---------|-------|
| `whisper_server_url` | `http://10.241.15.154:8001` |
| `whisper_server_model` | `large-v3-turbo` |
| `whisper_mode` | `remote` |

The app sends requests to `/v1/audio/transcriptions` with:
- Audio as WAV file (16kHz, mono, 16-bit PCM)
- `temperature: 0.0` (deterministic output)
- `no_speech_threshold: 0.8` (filter silence)
- `condition_on_previous_text: false` (prevent repetition)

## Additional Notes

- The `/v1/models` endpoint works correctly, confirming the server is running
- Only the `/v1/audio/transcriptions` endpoint fails due to missing ffmpeg
- No changes needed on the client/app side - this is purely a server dependency issue

## Contact

If you need the audio format changed or have questions about the API requests the app makes, refer to `tauri-app/src-tauri/src/whisper_server.rs` in the transcription app codebase.
