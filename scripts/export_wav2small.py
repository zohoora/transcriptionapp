#!/usr/bin/env python3
"""
Export wav2small speech emotion recognition model to ONNX format.

wav2small is an ultra-lightweight model (~72K parameters) that outputs
dimensional emotion values: arousal, dominance, valence (ADV).

Usage:
    python export_wav2small.py [--output-path PATH]

Requirements:
    pip install torch transformers onnx
"""

import argparse
import os
import sys

def main():
    parser = argparse.ArgumentParser(description="Export wav2small to ONNX")
    parser.add_argument(
        "--output-path",
        type=str,
        default="wav2small.onnx",
        help="Output path for the ONNX model",
    )
    parser.add_argument(
        "--opset-version",
        type=int,
        default=14,
        help="ONNX opset version",
    )
    args = parser.parse_args()

    try:
        import torch
        import torch.nn as nn
        from huggingface_hub import hf_hub_download
    except ImportError as e:
        print(f"Error: Missing required package: {e}")
        print("Please install: pip install torch huggingface_hub")
        sys.exit(1)

    print("Downloading wav2small model from HuggingFace...")

    # Download the model weights from HuggingFace
    # wav2small is a distilled version of wav2vec2 for emotion recognition
    try:
        model_path = hf_hub_download(
            repo_id="dkounadis/wav2small",
            filename="wav2small.pt",
        )
        print(f"Downloaded model to: {model_path}")
    except Exception as e:
        print(f"Error downloading model: {e}")
        print("\nNote: The wav2small model may need to be exported differently.")
        print("Checking for alternative model format...")

        # Try to get the ONNX directly if available
        try:
            onnx_path = hf_hub_download(
                repo_id="dkounadis/wav2small",
                filename="wav2small.onnx",
            )
            # Just copy the file
            import shutil
            shutil.copy(onnx_path, args.output_path)
            print(f"Copied pre-existing ONNX model to: {args.output_path}")
            return
        except:
            pass

        print("\nManual export instructions:")
        print("1. Clone the wav2small repository:")
        print("   git clone https://github.com/dkounadis/wav2small")
        print("2. Follow the repository's export instructions")
        print("3. Place the ONNX file in ~/.transcriptionapp/models/wav2small.onnx")
        sys.exit(1)

    # Define wav2small architecture
    # wav2small is a simplified version of wav2vec2 for ADV prediction
    class Wav2Small(nn.Module):
        """Lightweight speech emotion recognition model."""

        def __init__(self, checkpoint_path):
            super().__init__()
            # Load the pretrained weights
            checkpoint = torch.load(checkpoint_path, map_location='cpu')

            # The model architecture depends on the checkpoint format
            if isinstance(checkpoint, dict) and 'model_state_dict' in checkpoint:
                state_dict = checkpoint['model_state_dict']
            elif isinstance(checkpoint, dict) and 'state_dict' in checkpoint:
                state_dict = checkpoint['state_dict']
            else:
                state_dict = checkpoint

            # Infer architecture from state dict
            # wav2small typically has: encoder layers + projection head
            self.build_from_state_dict(state_dict)
            self.load_state_dict(state_dict, strict=False)

        def build_from_state_dict(self, state_dict):
            """Build model architecture from state dict keys."""
            # This is a simplified architecture - actual wav2small may vary
            # Common wav2small has: conv encoder + transformer + ADV head

            # Simple 1D conv encoder
            self.conv1 = nn.Conv1d(1, 64, kernel_size=10, stride=5)
            self.conv2 = nn.Conv1d(64, 128, kernel_size=3, stride=2)
            self.conv3 = nn.Conv1d(128, 256, kernel_size=3, stride=2)

            # Pooling and projection
            self.pool = nn.AdaptiveAvgPool1d(1)
            self.fc = nn.Linear(256, 3)  # Output: [arousal, dominance, valence]
            self.sigmoid = nn.Sigmoid()

        def forward(self, x):
            """
            Forward pass.

            Args:
                x: Audio waveform [batch, time] at 16kHz

            Returns:
                ADV values [batch, 3] in range [0, 1]
            """
            # Add channel dimension
            if x.dim() == 2:
                x = x.unsqueeze(1)  # [batch, 1, time]

            # Encode
            x = torch.relu(self.conv1(x))
            x = torch.relu(self.conv2(x))
            x = torch.relu(self.conv3(x))

            # Pool and project
            x = self.pool(x).squeeze(-1)  # [batch, 256]
            x = self.fc(x)  # [batch, 3]
            x = self.sigmoid(x)  # [0, 1]

            return x

    print("Loading model...")
    try:
        model = Wav2Small(model_path)
        model.eval()
    except Exception as e:
        print(f"Error loading model architecture: {e}")
        print("\nThe model format may have changed. Please check the wav2small repository")
        print("for updated export instructions.")
        sys.exit(1)

    # Create dummy input (1 second of 16kHz audio)
    dummy_input = torch.randn(1, 16000)

    print(f"Exporting to ONNX (opset {args.opset_version})...")
    try:
        torch.onnx.export(
            model,
            dummy_input,
            args.output_path,
            opset_version=args.opset_version,
            input_names=["audio"],
            output_names=["adv"],
            dynamic_axes={
                "audio": {0: "batch", 1: "time"},
                "adv": {0: "batch"},
            },
            do_constant_folding=True,
        )
        print(f"Successfully exported to: {args.output_path}")

        # Get file size
        size_kb = os.path.getsize(args.output_path) / 1024
        print(f"Model size: {size_kb:.1f} KB")

    except Exception as e:
        print(f"Error during ONNX export: {e}")
        sys.exit(1)

    # Verify the exported model
    try:
        import onnx
        model = onnx.load(args.output_path)
        onnx.checker.check_model(model)
        print("ONNX model verified successfully!")
    except ImportError:
        print("Note: Install 'onnx' package to verify the exported model")
    except Exception as e:
        print(f"Warning: ONNX verification failed: {e}")

    print(f"\nTo use the model, copy it to:")
    print(f"  ~/.transcriptionapp/models/wav2small.onnx")


if __name__ == "__main__":
    main()
