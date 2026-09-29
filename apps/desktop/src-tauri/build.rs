fn main() {
    // On Linux sherpa-onnx is linked as shared libs that the bundles install in
    // /usr/lib/talkr (see tauri.linux.conf.json). The binary lives in /usr/bin, in both
    // the .deb/.rpm and the AppImage, so this relative rpath finds them. `$ORIGIN` also
    // covers `cargo run`, where sherpa's build script copies the libs next to the binary.
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("linux") {
        println!("cargo:rustc-link-arg-bins=-Wl,-rpath,$ORIGIN/../lib/talkr:$ORIGIN");
    }

    // On Windows the installers bundle sherpa-onnx's DLLs from target/release (see
    // tauri.windows.conf.json). sherpa-rs-sys's build script puts them there, but Cargo
    // doesn't run it before this one, so say how to fix it instead of tauri-build's
    // bare "resource path doesn't exist".
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows")
        && !std::path::Path::new("target/release/sherpa-onnx-c-api.dll").exists()
    {
        panic!(
            "sherpa-onnx DLLs are missing from target/release. Stage them once with:\n\
             \n    cargo build --release -p sherpa-rs-sys\n\n\
             (run in apps/desktop/src-tauri), then build again."
        );
    }

    tauri_build::build()
}
