# Changelog

What changed in each Talkr release. The section for a version becomes its GitHub release notes and
the "What's new" text installed copies show when they offer the update
(`node scripts/release-notes.mjs X.Y.Z` prints it; the release workflow fails without one).

When releasing, rename **Unreleased** to the new version, e.g. `## 0.1.6 - 2026-10-08`.

## 0.1.8-2 - 2026-10-08

Beta 2: dictation on macOS and Linux, and a new look. Installed copies are not offered it as an
update.

### New
- **Dictation on macOS**: hold ⌃⌘ in any app (Talkr asks for Accessibility once). The pill shows
  over full-screen apps, text is pasted and checked, and remote desktops get real key presses.
- **Dictation on Linux, X11**: Talkr listens only for your shortcut (never other typing), pastes
  into the focused app (terminals included), and leaves your window manager's own shortcuts
  working.
- **Dictation on Linux, Wayland**: through the desktop's own portals (GNOME 48+, KDE Plasma 6,
  Hyprland): the desktop asks once for the shortcut and for keyboard access. Elsewhere, bind
  `talkr --dictate` to a key.
- A redesigned **Dictation** page: live status, guided setup, a pill preview, your system's own
  key names, and permission prompts that clear themselves.
- A redesigned **Appearance** section with a live preview of each theme before you pick it.
- The keys you press show live while you record a new shortcut.

### Fixed
- No more "vulkan-1.dll was not found" dialog on PCs without a Vulkan driver: Talkr goes
  straight to the CPU.
- Hold-to-talk with a key shortcut (like Ctrl + Alt + F8) no longer stops on its own.
- Closing the window quits Talkr when it is not kept in the tray, and opening Talkr again always
  shows the window.
- Ctrl + Win + Left (switching desktops) no longer starts a recording in press-twice mode.
- Short holds are timed from the key press, so they are not mistaken for taps.
- A shortcut another program already uses is named on the Dictation page.

## 0.1.8-1 - 2026-10-07

Beta: a pre-release for testing dictation. Installed copies are not offered it as an update.

### New
- **Dictate anywhere** (Windows): hold Ctrl + Win in any app, speak, and let go; your words are
  typed where your cursor is, transcribed on your computer. Tap the shortcut for hands-free
  dictation, press Esc to cancel, and Alt + Shift + V pastes the last dictation again.
- A floating pill shows the live waveform, which app the text goes to, and the result.
- Text lands in the field you started in: Talkr checks the field takes text, picks the best way
  in (straight into classic text boxes, pasting, or typing for remote desktops), reads the field
  back to confirm, and keeps your clipboard as it was. If it cannot insert, the text is copied
  and the pill says why.
- A new **Dictation** page: your own shortcuts (recorded from the keyboard), hold, tap or toggle,
  a separate model and language, microphone choice, custom vocabulary, replacements, filler-word
  removal, "new line" voice commands, per-app rules, and a practice box.
- Talkr can start with Windows and keep running in the tray while dictation is on.

### Changed
- Ctrl 3 now opens Dictation; Models and History moved to Ctrl 4 and Ctrl 5.
- The chosen microphone is also used for recordings in Transcribe.

## 0.1.7 - 2026-10-07

### New
- **Zoom** the whole interface with Ctrl + and Ctrl − (⌘ on macOS), or Ctrl and the mouse wheel;
  Ctrl 0 resets it. Settings → Appearance shows the level and remembers it.
- **Custom theme**: pick any accent colour, and Talkr builds matching light and dark palettes
  from it, keeping text readable.
- A redesigned Appearance section, with previews that follow your light or dark choice.

### Fixed
- The selected item in the sidebar no longer shows a stray bar beside its highlight.
- The sidebar's device card no longer clips its border or overflows long processor names.

## 0.1.6 - 2026-10-05

### New
- **Transcription quality** in Settings: Fast, Accurate, or Auto (the default), which is
  accurate for imported files and fast for recordings made in Talkr. Accurate makes fewer
  mistakes and takes about 1.5 times as long.
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
