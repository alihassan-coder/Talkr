// A GUI-subsystem binary on Windows, so starting it from the app never flashes a console.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    talkr_engine::logger::init();
    let stdin = std::io::stdin().lock();
    let stdout = std::io::stdout();
    talkr_engine::worker::serve(stdin, stdout, talkr_engine::worker::NativeEngines::default());
}
