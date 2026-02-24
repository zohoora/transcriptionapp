fn main() {
    // Register custom cfg for check-cfg lint
    println!("cargo::rustc-check-cfg=cfg(cfg_no_swift_bridge)");

    // Conditional Swift compilation for on-device LLM (Apple Foundation Models)
    #[cfg(target_os = "macos")]
    {
        let out_dir = std::env::var("OUT_DIR").unwrap();
        let swift_src = "swift/on_device_llm.swift";
        println!("cargo:rerun-if-changed={}", swift_src);

        if std::path::Path::new(swift_src).exists() {
            // Compile Swift to object file
            let status = std::process::Command::new("swiftc")
                .args([
                    "-emit-object",
                    "-parse-as-library",
                    "-O",
                    "-module-name",
                    "OnDeviceLLM",
                    "-o",
                    &format!("{}/on_device_llm.o", out_dir),
                    swift_src,
                ])
                .status();

            match status {
                Ok(s) if s.success() => {
                    println!("cargo:rustc-link-arg={}/on_device_llm.o", out_dir);
                    // Swift runtime libraries (system-provided on macOS 12.3+)
                    println!("cargo:rustc-link-lib=dylib=swiftCore");
                    // Swift Concurrency runtime (needed for Task{} / async-await bridging)
                    println!("cargo:rustc-link-lib=dylib=swift_Concurrency");
                    // Add rpath so dyld can find Swift runtime libs in the shared cache
                    println!("cargo:rustc-link-arg=-rpath");
                    println!("cargo:rustc-link-arg=/usr/lib/swift");
                }
                _ => {
                    println!(
                        "cargo:warning=Swift compilation failed — on-device LLM disabled"
                    );
                    println!("cargo:rustc-cfg=cfg_no_swift_bridge");
                }
            }
        } else {
            println!("cargo:rustc-cfg=cfg_no_swift_bridge");
        }
    }

    tauri_build::build();
}
