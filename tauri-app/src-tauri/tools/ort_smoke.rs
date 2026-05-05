//! ONNX Runtime smoke test — verify a bundled `libonnxruntime` dylib is
//! loadable by the compiled `ort` crate. Run by CI between bundle and publish
//! steps so a version-mismatched or wrong-arch dylib aborts the workflow
//! before `latest.json` is generated. The actual load test lives in
//! `transcription_app_lib::verify_ort_loadable` so CI and runtime exercise
//! the same code path.
//!
//! Usage:
//!   ORT_DYLIB_PATH=/path/to/libonnxruntime.<ver>.dylib \
//!       cargo run --bin ort_smoke --features diarization

#[cfg(feature = "diarization")]
fn main() -> std::process::ExitCode {
    use std::env;
    use std::process::ExitCode;

    /// `ORT_DYLIB_PATH` not set or empty.
    const EXIT_NO_DYLIB_PATH: u8 = 1;
    /// `Session::builder()` returned `Err` — runtime initialized, builder
    /// rejected the configuration (rare; usually an ABI break).
    const EXIT_BUILDER_ERR: u8 = 2;
    /// `Session::builder()` panicked on dlsym — wrong dylib (non-ONNX
    /// Runtime, badly stripped, or wrong arch).
    const EXIT_DLSYM_PANIC: u8 = 3;

    let dylib_path = match env::var("ORT_DYLIB_PATH") {
        Ok(p) if !p.is_empty() => p,
        _ => {
            eprintln!("ort_smoke: FAIL — ORT_DYLIB_PATH is not set or empty");
            eprintln!("hint: export ORT_DYLIB_PATH=<path-to-libonnxruntime.X.Y.Z.dylib>");
            return ExitCode::from(EXIT_NO_DYLIB_PATH);
        }
    };

    println!("ort_smoke: testing dylib at {}", dylib_path);

    match transcription_app_lib::verify_ort_loadable() {
        Ok(()) => {
            println!("ort_smoke: OK — Session::builder() succeeded");
            ExitCode::SUCCESS
        }
        Err(msg) if msg.contains("panicked") => {
            eprintln!("ort_smoke: FAIL — {}", msg);
            eprintln!(
                "hint: dylib is missing required ONNX Runtime symbols (e.g. OrtGetApiBase)."
            );
            eprintln!(
                "hint: confirm the bundled file is libonnxruntime, not a similarly-named"
            );
            eprintln!("hint: binary, and that lipo -info reports arm64.");
            ExitCode::from(EXIT_DLSYM_PANIC)
        }
        Err(msg) => {
            eprintln!("ort_smoke: FAIL — {}", msg);
            eprintln!(
                "hint: bundled libonnxruntime initialized but Session::builder() rejected it."
            );
            eprintln!(
                "hint: usually a runtime ABI break (see ort_sys / ort version pin)."
            );
            ExitCode::from(EXIT_BUILDER_ERR)
        }
    }
}

#[cfg(not(feature = "diarization"))]
fn main() -> std::process::ExitCode {
    eprintln!("ort_smoke: FAIL — built without the 'diarization' feature; ort crate not available");
    eprintln!("hint: cargo run --bin ort_smoke --features diarization");
    std::process::ExitCode::from(1)
}
