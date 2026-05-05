#!/bin/bash
# Setup ONNX Runtime for speaker diarization
# This script creates a Python venv with onnxruntime and exports ORT_DYLIB_PATH
#
# Single source of truth for the onnxruntime version: ORT_VERSION below.
# CI (.github/workflows/release.yml) sources this same value via:
#   ORT_VERSION=$(grep '^ORT_VERSION=' scripts/setup-ort.sh | cut -d'"' -f2)
# so local and CI builds stay in lockstep.

set -e

ORT_VENV="${ORT_VENV:-$HOME/.transcriptionapp/ort-venv}"
# Verified loadable under `ort 2.0.0-rc.10` via tools/ort_smoke.rs.
# ONNX Runtime is C-API forward-compatible across minors within 1.x, so newer
# 1.25.x revisions are safe; bump only after re-running ort_smoke against the
# new bundled dylib.
ORT_VERSION="1.25.1"
SENTINEL="$ORT_VENV/.ort-version"

# Re-create venv when the requested version drifts from what's already installed.
# Without this, existing dev boxes keep whatever version was first installed forever
# (the venv exists, so the install branch below is skipped) and the pin appears to
# work in CI but diverges locally.
if [ -d "$ORT_VENV" ] && [ "$(cat "$SENTINEL" 2>/dev/null)" != "$ORT_VERSION" ]; then
    echo "ONNX Runtime version drift ($(cat "$SENTINEL" 2>/dev/null || echo unknown) -> $ORT_VERSION); re-creating venv..."
    rm -rf "$ORT_VENV"
fi

if [ ! -d "$ORT_VENV" ]; then
    echo "Creating ONNX Runtime virtual environment at $ORT_VENV..."
    python3 -m venv "$ORT_VENV"
    "$ORT_VENV/bin/pip" install --quiet "onnxruntime==${ORT_VERSION}"
    echo "$ORT_VERSION" > "$SENTINEL"
    echo "ONNX Runtime ${ORT_VERSION} installed successfully"
fi

# Find the dylib
DYLIB_PATH=$(find "$ORT_VENV" -name "libonnxruntime.*.dylib" 2>/dev/null | head -1)

if [ -z "$DYLIB_PATH" ]; then
    echo "Error: Could not find libonnxruntime.*.dylib in $ORT_VENV"
    exit 1
fi

echo "$DYLIB_PATH"
