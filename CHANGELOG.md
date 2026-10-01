# Changelog

What changed in each Talkr release. The section for a version becomes its GitHub release notes and
the "What's new" text installed copies show when they offer the update
(`node scripts/release-notes.mjs X.Y.Z` prints it; the release workflow fails without one).

When releasing, rename **Unreleased** to the new version, e.g. `## 0.1.6 - 2026-10-08`.

## Unreleased

### New
- Save speech as **MP3**, **FLAC** or **WAV**. Talkr remembers the format you used last.
- Save transcripts as **plain text, Markdown, SRT or WebVTT subtitles, CSV or JSON**.
- Update notifications: Talkr checks for new versions at launch and every few hours, shows what
  is new, and lets you install now, later, or skip a version. Settings has a "Check now" button
  and a switch to turn automatic checks off.
- Playback speed (0.75× to 2×) in every player.
- In History, click a timestamp to play from that point, copy a transcript with timestamps, or
  open speech in Speak to hear it with another voice.
- Speak shows the word count and roughly how long the speech will be, and keeps what you typed
  when you switch screens.
- Press `?` for a list of keyboard shortcuts. Shortcut hints show ⌘ on macOS.

### Fixed
- Long transcriptions on slower computers are no longer stopped as "not responding".
- Cancelling a queued job no longer cancels the job that is running.
- Running out of memory on the GPU no longer turns the GPU off for good.
- Switching to the CPU stops the GPU engine right away, freeing video memory.
- Leaving Transcribe mid-recording now keeps and transcribes the recording.
- A double click on Play no longer marks the audio as missing.
- After switching speech models, Generate waits for the new model's voices.
- The update banner no longer covers the page's own buttons.
- A failed model list now offers Retry instead of saying nothing is installed.
- Error messages stay while you point at them, so they can be read and copied.
- macOS now asks for microphone access the proper way.
- Model names that end in a dot or match a Windows device name are refused.

## 0.1.5 - 2026-10-01

### Fixed
- Production hardening across the speech engine, audio playback, downloads and releases.
