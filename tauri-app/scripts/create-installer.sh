#!/bin/bash
# Creates a self-contained installer package for the Transcription App
# This bundles the app, models, config, and ONNX runtime into a single folder

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(dirname "$SCRIPT_DIR")"
BUILD_DIR="$PROJECT_DIR/src-tauri/target/debug/bundle/macos"
APP_NAME="Transcription App"
OUTPUT_DIR="$PROJECT_DIR/dist/TranscriptionApp-Installer"
MODELS_DIR="$HOME/.transcriptionapp/models"
CONFIG_FILE="$HOME/.transcriptionapp/config.json"

echo "=== Creating Transcription App Installer Package ==="

# 1. Build the app
echo ""
echo "Step 1: Building the app..."
cd "$PROJECT_DIR"
pnpm tauri build --debug 2>&1 | grep -E "(Compiling|Finished|Bundling|Built|Error)" || true

if [ ! -d "$BUILD_DIR/$APP_NAME.app" ]; then
    echo "ERROR: App build failed. $BUILD_DIR/$APP_NAME.app not found"
    exit 1
fi

# 2. Bundle ONNX Runtime
echo ""
echo "Step 2: Bundling ONNX Runtime..."
./scripts/bundle-ort.sh "$BUILD_DIR/$APP_NAME.app"

# 3. Create output directory structure
echo ""
echo "Step 3: Creating installer package..."
rm -rf "$OUTPUT_DIR"
mkdir -p "$OUTPUT_DIR/models"

# 4. Copy the app
echo "  - Copying app bundle..."
cp -R "$BUILD_DIR/$APP_NAME.app" "$OUTPUT_DIR/"

# 5. Copy required ONNX models
echo "  - Copying ONNX models..."
if [ -f "$MODELS_DIR/speaker_embedding.onnx" ]; then
    cp "$MODELS_DIR/speaker_embedding.onnx" "$OUTPUT_DIR/models/"
else
    echo "WARNING: speaker_embedding.onnx not found"
fi

if [ -f "$MODELS_DIR/gtcrn_simple.onnx" ]; then
    cp "$MODELS_DIR/gtcrn_simple.onnx" "$OUTPUT_DIR/models/"
else
    echo "WARNING: gtcrn_simple.onnx not found"
fi

if [ -f "$MODELS_DIR/yamnet.onnx" ]; then
    cp "$MODELS_DIR/yamnet.onnx" "$OUTPUT_DIR/models/"
else
    echo "WARNING: yamnet.onnx not found"
fi

# 6. Create default config (with your office settings pre-configured)
echo "  - Creating default config..."
cat > "$OUTPUT_DIR/config.json" << 'EOF'
{
  "schema_version": 1,
  "whisper_model": "large-v3-turbo",
  "language": "auto",
  "input_device_id": null,
  "output_format": "paragraphs",
  "vad_threshold": 0.5,
  "vad_pre_roll_ms": 300,
  "silence_to_flush_ms": 500,
  "max_utterance_ms": 25000,
  "model_path": null,
  "diarization_enabled": true,
  "max_speakers": 5,
  "speaker_similarity_threshold": 0.5,
  "diarization_model_path": null,
  "enhancement_enabled": true,
  "enhancement_model_path": null,
  "biomarkers_enabled": true,
  "yamnet_model_path": null,
  "preprocessing_enabled": false,
  "preprocessing_highpass_hz": 80,
  "preprocessing_agc_target_rms": 0.05,
  "llm_router_url": "http://10.241.15.154:8080",
  "llm_api_key": "ai-scribe-secret-key",
  "llm_client_id": "ai-scribe",
  "soap_model": "soap-model",
  "soap_model_fast": "soap-model",
  "fast_model": "fast-model",
  "medplum_server_url": "http://10.241.15.154:8103",
  "medplum_client_id": "af1464aa-e00c-4940-a32e-18d878b7911c",
  "medplum_auto_sync": true,
  "whisper_mode": "remote",
  "whisper_server_url": "http://10.241.15.154:8001",
  "whisper_server_model": "large-v3-turbo",
  "soap_detail_level": 5,
  "soap_format": "problem_based",
  "soap_custom_instructions": "",
  "auto_start_enabled": false,
  "greeting_sensitivity": 0.7,
  "min_speech_duration_ms": 3000,
  "auto_start_require_enrolled": false,
  "auto_start_required_role": null,
  "auto_end_enabled": true,
  "auto_end_silence_ms": 120000,
  "debug_storage_enabled": true
}
EOF

# 7. Create the install script
echo "  - Creating install script..."
cat > "$OUTPUT_DIR/install.command" << 'INSTALL_SCRIPT'
#!/bin/bash
# Transcription App Installer
# Double-click this file to install

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
APP_NAME="Transcription App"
DEST_DIR="$HOME/.transcriptionapp"

echo "=== Transcription App Installer ==="
echo ""

# Create destination directory
mkdir -p "$DEST_DIR/models"
mkdir -p "$DEST_DIR/logs"

# Copy models
echo "Installing models..."
if [ -d "$SCRIPT_DIR/models" ]; then
    cp -n "$SCRIPT_DIR/models/"*.onnx "$DEST_DIR/models/" 2>/dev/null || true
    # Create symlink for ecapa_tdnn.onnx -> speaker_embedding.onnx
    if [ -f "$DEST_DIR/models/speaker_embedding.onnx" ] && [ ! -e "$DEST_DIR/models/ecapa_tdnn.onnx" ]; then
        ln -s "$DEST_DIR/models/speaker_embedding.onnx" "$DEST_DIR/models/ecapa_tdnn.onnx"
    fi
fi

# Copy config (only if doesn't exist - don't overwrite user settings)
if [ ! -f "$DEST_DIR/config.json" ]; then
    echo "Installing default configuration..."
    cp "$SCRIPT_DIR/config.json" "$DEST_DIR/config.json"
else
    echo "Config already exists, keeping your settings."
fi

# Copy app to Applications
echo "Installing app to /Applications..."
if [ -d "/Applications/$APP_NAME.app" ]; then
    echo "  Removing old version..."
    rm -rf "/Applications/$APP_NAME.app"
fi
cp -R "$SCRIPT_DIR/$APP_NAME.app" "/Applications/"

# Set permissions
chmod -R 755 "/Applications/$APP_NAME.app"

echo ""
echo "=== Installation Complete ==="
echo ""
echo "Models installed to: $DEST_DIR/models/"
echo "Config installed to: $DEST_DIR/config.json"
echo "App installed to: /Applications/$APP_NAME.app"
echo ""
echo "You can now launch the app from /Applications or Spotlight."
echo ""

# Ask to launch
read -p "Launch the app now? (y/n) " -n 1 -r
echo
if [[ $REPLY =~ ^[Yy]$ ]]; then
    open "/Applications/$APP_NAME.app"
fi
INSTALL_SCRIPT

chmod +x "$OUTPUT_DIR/install.command"

# 8. Create a README
cat > "$OUTPUT_DIR/README.txt" << 'README'
Transcription App - Installation Instructions
=============================================

QUICK INSTALL:
Double-click "install.command" to install everything automatically.

MANUAL INSTALL:
1. Copy "Transcription App.app" to /Applications/
2. Create folder: ~/.transcriptionapp/models/
3. Copy all .onnx files from models/ folder to ~/.transcriptionapp/models/
4. Copy config.json to ~/.transcriptionapp/config.json
5. Create symlink: ln -s ~/.transcriptionapp/models/speaker_embedding.onnx ~/.transcriptionapp/models/ecapa_tdnn.onnx

REQUIREMENTS:
- macOS 12 or later (Apple Silicon or Intel)
- Network access to office servers (LLM, Whisper, Medplum)
- Microphone permission

FIRST RUN:
- Grant microphone permission when prompted
- The app uses remote Whisper server for transcription
- Login to Medplum if you want EMR sync

TROUBLESHOOTING:
- If "damaged app" error: Run in Terminal: xattr -cr "/Applications/Transcription App.app"
- If models not found: Check ~/.transcriptionapp/models/ has .onnx files
- If transcription fails: Verify network access to 10.241.15.154
README

# 9. Calculate sizes
echo ""
echo "=== Package Summary ==="
APP_SIZE=$(du -sh "$OUTPUT_DIR/$APP_NAME.app" | cut -f1)
MODELS_SIZE=$(du -sh "$OUTPUT_DIR/models" | cut -f1)
TOTAL_SIZE=$(du -sh "$OUTPUT_DIR" | cut -f1)

echo "App size: $APP_SIZE"
echo "Models size: $MODELS_SIZE"
echo "Total package size: $TOTAL_SIZE"
echo ""
echo "Installer package created at:"
echo "  $OUTPUT_DIR"
echo ""
echo "To deploy to another computer:"
echo "  1. Copy the entire 'TranscriptionApp-Installer' folder"
echo "  2. Double-click 'install.command' on the target computer"
