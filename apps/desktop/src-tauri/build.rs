fn main() {
    // On Linux sherpa-onnx is linked as shared libs that the bundles install in
    // /usr/lib/talkr (see tauri.linux.conf.json). The binary lives in /usr/bin, in both
    // the .deb/.rpm and the AppImage, so this relative rpath finds them. `$ORIGIN` also
    // covers `cargo run`, where sherpa's build script copies the libs next to the binary.
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("linux") {
        println!("cargo:rustc-link-arg-bins=-Wl,-rpath,$ORIGIN/../lib/talkr:$ORIGIN");
    }

    tauri_build::build()
}
