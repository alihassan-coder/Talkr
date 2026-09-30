fn main() {
    // The speech engine is a sidecar (bundle.externalBin), and tauri-build insists it exists
    // even for `cargo check`. Say how to make it instead of a bare "resource path doesn't exist".
    let triple = std::env::var("TARGET").unwrap_or_default();
    let suffix = if triple.contains("windows") { ".exe" } else { "" };
    let engine = format!("binaries/talkr-engine-{}{}", triple, suffix);
    if !std::path::Path::new(&engine).exists() {
        panic!(
            "{} is missing. Build the engine first (in apps/desktop):\n\n    \
             node scripts/build-engine.mjs --debug\n\n\
             Drop --debug for release builds.",
            engine
        );
    }

    // On Windows the installers bundle sherpa-onnx's DLLs from target/release (see
    // tauri.windows.conf.json), where the release engine build puts them.
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows")
        && std::env::var("PROFILE").as_deref() == Ok("release")
        && !std::path::Path::new("target/release/sherpa-onnx-c-api.dll").exists()
    {
        panic!(
            "sherpa-onnx DLLs are missing from target/release. Build the engine first \
             (in apps/desktop):\n\n    node scripts/build-engine.mjs\n"
        );
    }

    tauri_build::build()
}
