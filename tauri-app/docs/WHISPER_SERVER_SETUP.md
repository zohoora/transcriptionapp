# Whisper Server Setup Guide

> **Note (Feb 2026)**: The transcription app now connects through the **STT Router** (see ADR-0020), not directly to this Whisper server. The STT Router wraps this server and provides WebSocket streaming with alias-based routing. This guide documents the **backend Whisper server** setup. The app's `whisper_server_url` config points to the STT Router, which forwards to this server.

This guide is for setting up the Whisper transcription backend on the Mac that already runs Medplum and the LLM router.

## Overview

- **Server**: faster-whisper-server (Speaches) - OpenAI-compatible API
- **Model**: large-v3-turbo (recommended for speed/quality balance)
- **Port**: 8000 (STT Router on port 8001 forwards to this)
- **Existing services on this Mac**:
  - Medplum server (FHIR EMR)
  - LLM router (OpenAI-compatible API for SOAP notes)
  - STT Router (WebSocket streaming gateway on port 8001)

## Prerequisites

- macOS with Apple Silicon (M1/M2/M3) - 16GB+ RAM recommended
- Python 3.10+ installed
- Homebrew installed

## Installation Options

### Option A: Native Installation (Recommended for Mac)

Native installation gives better performance on Apple Silicon by using Metal acceleration.

#### 1. Create a virtual environment

```bash
cd ~
mkdir whisper-server
cd whisper-server
python3 -m venv venv
source venv/bin/activate
```

#### 2. Install faster-whisper-server (Speaches)

```bash
pip install speaches
```

#### 3. Download the model (first run will auto-download, but you can pre-download)

```bash
# Optional: Pre-download the model
pip install faster-whisper
python -c "from faster_whisper import WhisperModel; WhisperModel('large-v3-turbo', device='cpu', compute_type='int8')"
```

#### 4. Create a launch script

Create `~/whisper-server/start.sh`:

```bash
#!/bin/bash
cd ~/whisper-server
source venv/bin/activate

# Start the server
# - Host on all interfaces so other devices on LAN can connect
# - Use port 8000
# - Use large-v3-turbo model
# - Use int8 quantization for faster CPU inference
speaches --host 0.0.0.0 --port 8000
```

Make it executable:

```bash
chmod +x ~/whisper-server/start.sh
```

#### 5. Test the server

Start the server:

```bash
~/whisper-server/start.sh
```

In another terminal, test with curl:

```bash
# Check if server is running
curl http://localhost:8000/v1/models

# Test transcription with a sample audio file
curl -X POST http://localhost:8000/v1/audio/transcriptions \
  -F "file=@test.wav" \
  -F "model=large-v3-turbo"
```

### Option B: Docker Installation

Docker is simpler but won't use Metal acceleration on Mac.

```bash
# CPU-only (no GPU acceleration in Docker on Mac)
docker run -d \
  --name whisper-server \
  -p 8000:8000 \
  -v whisper-models:/root/.cache/huggingface \
  ghcr.io/speaches-ai/speaches:latest-cpu
```

Check logs:

```bash
docker logs -f whisper-server
```

## Running as a Background Service (Native)

### Using launchd (Recommended for macOS)

Create `~/Library/LaunchAgents/com.whisper.server.plist`:

```xml
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>com.whisper.server</string>
    <key>ProgramArguments</key>
    <array>
        <string>/bin/bash</string>
        <string>-c</string>
        <string>cd ~/whisper-server && source venv/bin/activate && speaches --host 0.0.0.0 --port 8000</string>
    </array>
    <key>RunAtLoad</key>
    <true/>
    <key>KeepAlive</key>
    <true/>
    <key>StandardOutPath</key>
    <string>/tmp/whisper-server.log</string>
    <key>StandardErrorPath</key>
    <string>/tmp/whisper-server.error.log</string>
</dict>
</plist>
```

Load the service:

```bash
launchctl load ~/Library/LaunchAgents/com.whisper.server.plist
```

Control commands:

```bash
# Start
launchctl start com.whisper.server

# Stop
launchctl stop com.whisper.server

# Unload (disable)
launchctl unload ~/Library/LaunchAgents/com.whisper.server.plist

# Check status
launchctl list | grep whisper

# View logs
tail -f /tmp/whisper-server.log
```

## Configuration

### Default Settings

The transcription app expects:
- **URL**: `http://<server-ip>:8000`
- **Model**: `large-v3-turbo`
- **API**: OpenAI-compatible `/v1/audio/transcriptions`

### Available Models

| Model | Size | Speed | Quality | RAM |
|-------|------|-------|---------|-----|
| `large-v3-turbo` | 1.5GB | Fast | Excellent | ~6GB |
| `large-v3` | 3GB | Slow | Best | ~10GB |
| `distil-large-v3` | 1.5GB | Very Fast | Great | ~4GB |
| `medium` | 1.5GB | Fast | Good | ~3GB |

### Changing the Default Model

Edit the start script to specify a different model:

```bash
speaches --host 0.0.0.0 --port 8000 --model large-v3-turbo
```

Or set via environment variable:

```bash
export WHISPER_MODEL=large-v3-turbo
speaches --host 0.0.0.0 --port 8000
```

## Network Configuration

### Find the Server IP

On the Mac running the server:

```bash
# Get the local IP address
ipconfig getifaddr en0
```

This will return something like `192.168.50.149`.

### Firewall

Ensure port 8000 is accessible:

```bash
# Check if firewall is enabled
sudo /usr/libexec/ApplicationFirewall/socketfilterfw --getglobalstate

# If enabled, add exception for the port (or disable firewall for local network)
```

### Test from Client Device

From the device running the transcription app:

```bash
curl http://192.168.50.149:8000/v1/models
```

You should see a list of available models.

## API Reference

### List Models

```bash
GET /v1/models
```

Response:
```json
{
  "data": [
    {"id": "large-v3-turbo", "object": "model"},
    {"id": "large-v3", "object": "model"}
  ]
}
```

### Transcribe Audio

```bash
POST /v1/audio/transcriptions
Content-Type: multipart/form-data

file: <audio file (WAV, MP3, etc.)>
model: large-v3-turbo
language: en (optional, auto-detect if omitted)
```

Response:
```json
{
  "text": "The transcribed text appears here."
}
```

## Integration with Transcription App

The app connects through the **STT Router**, not directly to this Whisper server.

In `~/.transcriptionapp/config.json`:

```json
{
  "whisper_server_url": "http://10.241.15.154:8001",
  "stt_alias": "medical-streaming",
  "stt_postprocess": true
}
```

The STT Router on port 8001 handles WebSocket streaming and forwards to this Whisper backend. See ADR-0020 for protocol details.

## Troubleshooting

### Server won't start

Check if port 8000 is already in use:

```bash
lsof -i :8000
```

If another service is using it, change the port:

```bash
speaches --host 0.0.0.0 --port 8001
```

### Out of memory

Try a smaller model:

```bash
speaches --host 0.0.0.0 --port 8000 --model medium
```

Or use int8 quantization (already default, but verify):

```bash
speaches --host 0.0.0.0 --port 8000 --compute-type int8
```

### Slow transcription

1. Ensure no other heavy processes are running
2. Check Activity Monitor for CPU/memory usage
3. Consider using `distil-large-v3` for faster processing

### Connection refused from client

1. Check firewall settings
2. Verify server is running: `curl http://localhost:8000/v1/models`
3. Verify IP address is correct
4. Ensure both devices are on the same network

### Model download fails

Manually download:

```bash
source ~/whisper-server/venv/bin/activate
python -c "from faster_whisper import WhisperModel; WhisperModel('large-v3-turbo', device='cpu', compute_type='int8')"
```

## Services Summary

After setup, this Mac will run:

| Service | Port | Purpose |
|---------|------|---------|
| Medplum | 8103 | FHIR EMR server |
| LLM Router | 8080 | OpenAI-compatible API for SOAP notes |
| Whisper | 8000 | Speech-to-text backend |
| STT Router | 8001 | WebSocket streaming gateway (app connects here) |

## Maintenance

### Update the server

```bash
source ~/whisper-server/venv/bin/activate
pip install --upgrade speaches faster-whisper
```

### Clear model cache

```bash
rm -rf ~/.cache/huggingface/hub/models--Systran--faster-whisper-*
```

### View logs

```bash
# If using launchd
tail -f /tmp/whisper-server.log

# If running manually
# Logs appear in the terminal
```
