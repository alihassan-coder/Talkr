//! Cues through PlaySound: asynchronous and lighter than opening an audio stream.

use windows::core::PCWSTR;
use windows::Win32::Media::Audio::{PlaySoundW, SND_ASYNC, SND_MEMORY, SND_NODEFAULT};
use crate::dictation::backend::Cue;
use crate::dictation::cues;

pub fn play(cue: Cue) {
    let data = cues::wav(cue);
    // SAFETY: SND_MEMORY reads the WAV image from `data`, which is 'static, so it outlives the
    // asynchronous playback.
    unsafe {
        let _ = PlaySoundW(PCWSTR(data.as_ptr() as *const u16), None, SND_MEMORY | SND_ASYNC | SND_NODEFAULT);
    }
}
