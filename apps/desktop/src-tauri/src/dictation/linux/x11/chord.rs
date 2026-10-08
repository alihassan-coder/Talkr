//! What the shortcut listener decides, apart from any X I/O, so it can be tested key by key.
//!
//! Talkr never reads the keyboard in general on X11. The server hands it keys through passive
//! grabs only: the shortcut's own key combinations ([`grabs`]), each of which, when pressed,
//! gives Talkr the keyboard until that key is released. While such a grab lasts, every key event
//! comes here first and is either kept ([`Verdict::Keep`]) or given back to the focused app
//! untouched ([`Verdict::Replay`], which also ends the grab). Recording a new shortcut in the
//! settings takes the whole keyboard briefly instead ([`Capture`]).

use super::keys::{self, VK_ESCAPE, VK_UNKNOWN};
use crate::dictation::settings::Shortcut;
use crate::dictation::HotkeyEvent;

pub const CTRL: u8 = 1;
pub const SHIFT: u8 = 2;
pub const ALT: u8 = 4;
pub const WIN: u8 = 8;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Chord {
    pub mods: u8,
    pub key: Option<u16>,
}

impl Chord {
    pub fn of(s: &Shortcut) -> Self {
        let mut mods = 0;
        for (on, bit) in [(s.ctrl, CTRL), (s.shift, SHIFT), (s.alt, ALT), (s.win, WIN)] {
            if on {
                mods |= bit;
            }
        }
        Self { mods, key: s.key.filter(|&k| k != 0) }
    }
}

/// What the listener acts on.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Config {
    pub dictate: Option<Chord>,
    pub paste: Option<Chord>,
    /// A dictation is recording: Escape cancels it.
    pub recording: bool,
}

pub fn modifier_bit(vk: u16) -> u8 {
    match vk {
        keys::VK_LCONTROL | keys::VK_RCONTROL => CTRL,
        keys::VK_LSHIFT | keys::VK_RSHIFT => SHIFT,
        keys::VK_LMENU | keys::VK_RMENU => ALT,
        keys::VK_LWIN | keys::VK_RWIN => WIN,
        _ => 0,
    }
}

fn modifier_keys(bit: u8) -> [u16; 2] {
    match bit {
        CTRL => [keys::VK_LCONTROL, keys::VK_RCONTROL],
        SHIFT => [keys::VK_LSHIFT, keys::VK_RSHIFT],
        ALT => [keys::VK_LMENU, keys::VK_RMENU],
        _ => [keys::VK_LWIN, keys::VK_RWIN],
    }
}

/// A passive grab: `vk` pressed while exactly `mods` are held.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Grab {
    pub vk: u16,
    pub mods: u8,
}

/// The grabs that deliver `config`'s shortcuts, and nothing else. A key chord grabs its key with
/// its modifiers. A chord of modifiers alone grabs each of its modifier keys with the others held
/// (Super with Ctrl, Ctrl with Super), so whichever is pressed last completes it. Escape is
/// grabbed only while recording.
pub fn grabs(config: &Config) -> Vec<Grab> {
    let mut out = Vec::new();
    for chord in [config.dictate, config.paste].into_iter().flatten() {
        match chord.key {
            Some(vk) => out.push(Grab { vk, mods: chord.mods }),
            // One modifier alone would take that modifier from every app; settings refuse it.
            None if chord.mods.count_ones() >= 2 => {
                for bit in [CTRL, SHIFT, ALT, WIN] {
                    if chord.mods & bit != 0 {
                        out.extend(modifier_keys(bit).map(|vk| Grab { vk, mods: chord.mods & !bit }));
                    }
                }
            }
            None => {}
        }
    }
    if config.recording {
        out.push(Grab { vk: VK_ESCAPE, mods: 0 });
    }
    out.sort();
    out.dedup();
    out
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    /// The event stays with Talkr; the next one is delivered here too.
    Keep,
    /// Give the event to the focused app as if Talkr had never grabbed it. Ends the grab.
    Replay,
}

#[derive(Debug, Default)]
pub struct Machine {
    /// The key whose press started the current grab; the grab ends when it is released.
    grab: Option<u16>,
    /// Keys whose press was kept from the app during this grab: their repeats and release are
    /// kept too, or the app would see a key go up that never went down.
    kept: Vec<u16>,
    dictate_active: bool,
    paste_active: bool,
}

fn remove(list: &mut Vec<u16>, vk: u16) -> bool {
    match list.iter().position(|&k| k == vk) {
        Some(i) => {
            list.swap_remove(i);
            true
        }
        None => false,
    }
}

impl Machine {
    /// A grab of Talkr's is active.
    pub fn grabbed(&self) -> bool {
        self.grab.is_some()
    }

    /// One key event delivered through a grab. `mods` are the modifiers held before it (the X
    /// event's state). Events go to `out`.
    pub fn key(&mut self, config: &Config, vk: u16, down: bool, mods: u8, out: &mut Vec<HotkeyEvent>) -> Verdict {
        let bit = modifier_bit(vk);
        let after = match (bit, down) {
            (0, _) => mods,
            (_, true) => mods | bit,
            (_, false) => mods & !bit,
        };
        let repeat = down && self.kept.contains(&vk);
        if self.grab.is_none() {
            if !down {
                // Releases never start a grab: not Talkr's.
                return Verdict::Replay;
            }
            self.grab = Some(vk);
        }

        let mut keep = false;
        if vk == VK_ESCAPE && config.recording {
            if down && !repeat {
                out.push(HotkeyEvent::Cancel);
            }
            keep = true;
        } else {
            if let Some(chord) = config.dictate {
                keep |= self.dictate(chord, vk, down, repeat, mods, after, out);
            }
            if let Some(chord) = config.paste {
                keep |= self.paste_last(chord, vk, down, repeat, mods, after, out);
            }
        }

        let verdict = if down {
            // While a kept key that types (Space of Ctrl + Alt + Space) is held, other keys stay
            // here too: giving one back would end the grab and send that key's repeats to the app.
            if keep || repeat || self.kept.iter().any(|&k| modifier_bit(k) == 0) {
                if !self.kept.contains(&vk) {
                    self.kept.push(vk);
                }
                Verdict::Keep
            } else {
                Verdict::Replay
            }
        } else if remove(&mut self.kept, vk) || !self.kept.is_empty() {
            Verdict::Keep
        } else {
            Verdict::Replay
        };

        if verdict == Verdict::Replay || (!down && self.grab == Some(vk)) {
            self.end_grab(out);
        }
        verdict
    }

    fn end_grab(&mut self, out: &mut Vec<HotkeyEvent>) {
        if self.dictate_active {
            // Its release can no longer be seen: report it now rather than never.
            out.push(HotkeyEvent::DictateUp);
        }
        *self = Self::default();
    }

    #[allow(clippy::too_many_arguments)]
    fn dictate(&mut self, chord: Chord, vk: u16, down: bool, repeat: bool, mods: u8, after: u8, out: &mut Vec<HotkeyEvent>) -> bool {
        let bit = modifier_bit(vk);
        match chord.key {
            Some(key) => {
                if vk == key {
                    if down {
                        if self.dictate_active && repeat {
                            return true;
                        }
                        self.dictate_active = false;
                        if !repeat && mods == chord.mods {
                            self.dictate_active = true;
                            out.push(HotkeyEvent::DictateDown);
                            return true;
                        }
                    } else if self.dictate_active {
                        self.dictate_active = false;
                        out.push(HotkeyEvent::DictateUp);
                    }
                } else if self.dictate_active && !down && bit != 0 && after & chord.mods != chord.mods {
                    // A modifier of the shortcut let go first.
                    self.dictate_active = false;
                    out.push(HotkeyEvent::DictateUp);
                }
                false
            }
            None => {
                if bit != 0 {
                    if down && !repeat {
                        if self.dictate_active {
                            if after != chord.mods {
                                // Another modifier joined: that is a different shortcut.
                                self.dictate_active = false;
                                out.push(HotkeyEvent::Interrupted);
                            }
                        } else if after == chord.mods && chord.mods.count_ones() >= 2 {
                            self.dictate_active = true;
                            out.push(HotkeyEvent::DictateDown);
                            return true;
                        }
                    } else if !down && self.dictate_active && after & chord.mods != chord.mods {
                        self.dictate_active = false;
                        out.push(HotkeyEvent::DictateUp);
                    }
                } else if down && !repeat && self.dictate_active {
                    // Ctrl + Super + Left and friends: the desktop's shortcut, not dictation.
                    self.dictate_active = false;
                    out.push(HotkeyEvent::Interrupted);
                }
                false
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn paste_last(&mut self, chord: Chord, vk: u16, down: bool, repeat: bool, mods: u8, after: u8, out: &mut Vec<HotkeyEvent>) -> bool {
        match chord.key {
            Some(key) if vk == key => {
                if down && !repeat && mods == chord.mods {
                    self.paste_active = true;
                    out.push(HotkeyEvent::PasteLast);
                    return true;
                }
                if down && self.paste_active {
                    return true;
                }
                if !down {
                    self.paste_active = false;
                }
                false
            }
            Some(_) => false,
            None => {
                if modifier_bit(vk) != 0
                    && down
                    && !repeat
                    && !self.paste_active
                    && after == chord.mods
                    && chord.mods.count_ones() >= 2
                {
                    self.paste_active = true;
                    out.push(HotkeyEvent::PasteLast);
                    return true;
                }
                if !down && after & chord.mods != chord.mods {
                    self.paste_active = false;
                }
                false
            }
        }
    }
}

/// Recording a new shortcut while the keyboard is grabbed for it: the combination is reported
/// once every key pressed since is up again. Escape alone cancels.
#[derive(Debug, Default)]
pub struct Capture {
    down: Vec<u16>,
    chord: Option<Chord>,
}

impl Capture {
    pub fn key(&mut self, vk: u16, down: bool) -> Option<HotkeyEvent> {
        if down {
            if self.down.contains(&vk) {
                return None; // auto-repeat
            }
            self.down.push(vk);
            if vk == VK_ESCAPE && self.chord.is_none() {
                return Some(HotkeyEvent::CaptureCancelled);
            }
            if vk == VK_UNKNOWN {
                return None;
            }
            let chord = self.chord.get_or_insert(Chord { mods: 0, key: None });
            let bit = modifier_bit(vk);
            chord.mods |= bit;
            if bit == 0 {
                chord.key = Some(vk);
            }
            return None;
        }
        // Keys held before recording began are not part of it.
        if !remove(&mut self.down, vk) || !self.down.is_empty() {
            return None;
        }
        let chord = self.chord.take()?;
        Some(HotkeyEvent::Captured(Shortcut {
            ctrl: chord.mods & CTRL != 0,
            shift: chord.mods & SHIFT != 0,
            alt: chord.mods & ALT != 0,
            win: chord.mods & WIN != 0,
            key: chord.key,
            key_label: chord.key.map(keys::label),
        }))
    }
}

#[cfg(test)]
mod tests {
    //! The same cases as the Windows hook's tests (win/hook.rs), against a simulated X server:
    //! it delivers an event only through a grab, as the real one does.
    use super::*;
    use crate::dictation::HotkeyEvent as E;
    use keys::{VK_LCONTROL as LCTRL, VK_LMENU as LALT, VK_LSHIFT as LSHIFT, VK_LWIN as LWIN};

    const LEFT: u16 = 0x25;
    const SPACE: u16 = 0x20;
    const KEY_A: u16 = 0x41;
    const KEY_V: u16 = 0x56;

    #[derive(Debug, PartialEq, Eq)]
    enum Went {
        /// No grab matched: the focused app got it.
        App,
        Kept,
        Replayed,
    }

    struct Server {
        config: Config,
        machine: Machine,
        /// Keys physically held.
        held: Vec<u16>,
        events: Vec<HotkeyEvent>,
    }

    impl Server {
        fn new(dictate: Option<Shortcut>, paste: Option<Shortcut>) -> Self {
            let config = Config { dictate: dictate.as_ref().map(Chord::of), paste: paste.as_ref().map(Chord::of), recording: false };
            Self { config, machine: Machine::default(), held: Vec::new(), events: Vec::new() }
        }

        fn mods(&self) -> u8 {
            self.held.iter().fold(0, |m, &k| m | modifier_bit(k))
        }

        fn key(&mut self, vk: u16, down: bool) -> Went {
            let mods = self.mods();
            if down && !self.held.contains(&vk) {
                self.held.push(vk);
            } else if !down {
                remove(&mut self.held, vk);
            }
            let delivered = self.machine.grabbed() || (down && grabs(&self.config).contains(&Grab { vk, mods }));
            if !delivered {
                return Went::App;
            }
            match self.machine.key(&self.config, vk, down, mods, &mut self.events) {
                Verdict::Keep => Went::Kept,
                Verdict::Replay => Went::Replayed,
            }
        }

        fn events(&mut self) -> Vec<HotkeyEvent> {
            std::mem::take(&mut self.events)
        }
    }

    fn ctrl_space() -> Shortcut {
        Shortcut { ctrl: true, shift: false, alt: false, win: false, key: Some(SPACE), key_label: None }
    }

    #[test]
    fn grabs_cover_exactly_the_shortcuts() {
        let config = Config { dictate: Some(Chord::of(&Shortcut::ctrl_win())), paste: Some(Chord::of(&Shortcut::alt_shift_v())), recording: false };
        assert_eq!(
            grabs(&config),
            vec![
                Grab { vk: KEY_V, mods: ALT | SHIFT },
                Grab { vk: LWIN, mods: CTRL },
                Grab { vk: keys::VK_RWIN, mods: CTRL },
                Grab { vk: LCTRL, mods: WIN },
                Grab { vk: keys::VK_RCONTROL, mods: WIN },
            ]
        );
        let recording = Config { recording: true, ..config.clone() };
        assert!(grabs(&recording).contains(&Grab { vk: VK_ESCAPE, mods: 0 }));
        assert!(!grabs(&config).iter().any(|g| g.vk == VK_ESCAPE), "Escape is only grabbed while recording");
        // A lone modifier would take it from every app.
        let lone = Config { dictate: Some(Chord { mods: CTRL, key: None }), ..Config::default() };
        assert!(grabs(&lone).is_empty());
        let three = Config { dictate: Some(Chord { mods: CTRL | ALT | WIN, key: None }), ..Config::default() };
        assert!(grabs(&three).contains(&Grab { vk: LALT, mods: CTRL | WIN }));
        assert_eq!(grabs(&three).len(), 6);
    }

    #[test]
    fn modifier_chord_reports_press_and_release() {
        let mut x = Server::new(Some(Shortcut::ctrl_win()), None);
        assert_eq!(x.key(LCTRL, true), Went::App);
        assert_eq!(x.key(LWIN, true), Went::Kept);
        assert_eq!(x.events(), vec![E::DictateDown]);
        x.key(LWIN, true);
        assert!(x.events().is_empty(), "auto-repeat changes nothing");
        assert_eq!(x.key(LWIN, false), Went::Kept);
        assert_eq!(x.events(), vec![E::DictateUp]);
        assert_eq!(x.key(LCTRL, false), Went::App);
        assert!(x.events().is_empty());
        assert!(!x.machine.grabbed());
    }

    #[test]
    fn either_modifier_can_complete_the_chord() {
        let mut x = Server::new(Some(Shortcut::ctrl_win()), None);
        assert_eq!(x.key(LWIN, true), Went::App);
        assert_eq!(x.key(LCTRL, true), Went::Kept);
        assert_eq!(x.events(), vec![E::DictateDown]);
        // Super let go first: the chord ends; the grab lasts until Ctrl is up.
        assert_eq!(x.key(LWIN, false), Went::Kept);
        assert_eq!(x.events(), vec![E::DictateUp]);
        assert_eq!(x.key(LCTRL, false), Went::Kept);
        assert!(!x.machine.grabbed());
    }

    #[test]
    fn desktop_shortcuts_interrupt_and_reach_the_app() {
        let mut x = Server::new(Some(Shortcut::ctrl_win()), None);
        x.key(LCTRL, true);
        x.key(LWIN, true);
        assert_eq!(x.key(LEFT, true), Went::Replayed, "the arrow goes back to the desktop");
        assert_eq!(x.events(), vec![E::DictateDown, E::Interrupted]);
        // Pressing the arrow again while still holding does nothing.
        assert_eq!(x.key(LEFT, false), Went::App);
        assert_eq!(x.key(LEFT, true), Went::App);
        x.key(LEFT, false);
        x.key(LWIN, false);
        x.key(LCTRL, false);
        assert!(x.events().is_empty());
        // Released fully: works again.
        x.key(LCTRL, true);
        x.key(LWIN, true);
        assert_eq!(x.events(), vec![E::DictateDown]);
    }

    #[test]
    fn another_modifier_interrupts() {
        let mut x = Server::new(Some(Shortcut::ctrl_win()), None);
        x.key(LCTRL, true);
        x.key(LWIN, true);
        assert_eq!(x.key(LSHIFT, true), Went::Replayed);
        assert_eq!(x.events(), vec![E::DictateDown, E::Interrupted]);
    }

    #[test]
    fn chord_only_fires_on_exact_modifiers() {
        let mut x = Server::new(Some(Shortcut::ctrl_win()), None);
        x.key(LSHIFT, true);
        x.key(LCTRL, true);
        assert_eq!(x.key(LWIN, true), Went::App, "Ctrl + Shift + Super is another shortcut");
        assert!(x.events().is_empty());
    }

    #[test]
    fn key_chord_keeps_its_key() {
        let mut x = Server::new(Some(ctrl_space()), None);
        assert_eq!(x.key(LCTRL, true), Went::App);
        assert_eq!(x.key(SPACE, true), Went::Kept, "kept from the app");
        assert_eq!(x.key(SPACE, true), Went::Kept, "repeats too");
        assert_eq!(x.key(SPACE, false), Went::Kept, "and the release");
        assert_eq!(x.key(LCTRL, false), Went::App);
        assert_eq!(x.events(), vec![E::DictateDown, E::DictateUp]);
        // Space alone is just typing.
        assert_eq!(x.key(SPACE, true), Went::App);
        assert_eq!(x.key(SPACE, false), Went::App);
        assert!(x.events().is_empty());
    }

    #[test]
    fn a_held_key_stays_kept_after_its_modifier_lets_go() {
        let mut x = Server::new(Some(ctrl_space()), None);
        x.key(LCTRL, true);
        assert_eq!(x.key(SPACE, true), Went::Kept);
        assert_eq!(x.key(LCTRL, false), Went::Kept);
        assert_eq!(x.events(), vec![E::DictateDown, E::DictateUp]);
        // Space still held: its repeats and release stay with Talkr.
        assert_eq!(x.key(SPACE, true), Went::Kept);
        assert_eq!(x.key(SPACE, false), Went::Kept);
        assert!(x.events().is_empty());
        // And Space works normally afterwards.
        assert_eq!(x.key(SPACE, true), Went::App);
        x.key(SPACE, false);
    }

    #[test]
    fn typing_during_a_key_chord_is_kept_but_does_not_end_it() {
        let mut x = Server::new(Some(ctrl_space()), None);
        x.key(LCTRL, true);
        x.key(SPACE, true);
        assert_eq!(x.key(KEY_A, true), Went::Kept);
        assert_eq!(x.key(KEY_A, false), Went::Kept);
        assert_eq!(x.events(), vec![E::DictateDown]);
        x.key(SPACE, false);
        assert_eq!(x.events(), vec![E::DictateUp]);
    }

    #[test]
    fn escape_cancels_only_while_recording() {
        let mut x = Server::new(Some(Shortcut::ctrl_win()), None);
        assert_eq!(x.key(VK_ESCAPE, true), Went::App);
        x.key(VK_ESCAPE, false);
        x.config.recording = true;
        assert_eq!(x.key(VK_ESCAPE, true), Went::Kept);
        for _ in 0..30 {
            assert_eq!(x.key(VK_ESCAPE, true), Went::Kept);
        }
        assert_eq!(x.key(VK_ESCAPE, false), Went::Kept);
        assert_eq!(x.events(), vec![E::Cancel]);
        x.config.recording = false;
        assert_eq!(x.key(VK_ESCAPE, true), Went::App);
        assert_eq!(x.key(VK_ESCAPE, false), Went::App, "a later Escape reaches the app");
        assert!(x.events().is_empty());
    }

    #[test]
    fn escape_cancels_while_the_chord_is_held() {
        let mut x = Server::new(Some(Shortcut::ctrl_win()), None);
        x.config.recording = true;
        x.key(LCTRL, true);
        x.key(LWIN, true);
        assert_eq!(x.key(VK_ESCAPE, true), Went::Kept);
        assert_eq!(x.events(), vec![E::DictateDown, E::Cancel]);
    }

    #[test]
    fn a_stale_grab_gives_the_key_back() {
        // Recording ended between the press and its delivery.
        let mut x = Server::new(Some(Shortcut::ctrl_win()), None);
        x.config.recording = true;
        let mods = x.mods();
        x.held.push(VK_ESCAPE);
        x.config.recording = false;
        assert_eq!(x.machine.key(&x.config, VK_ESCAPE, true, mods, &mut x.events), Verdict::Replay);
        assert!(!x.machine.grabbed());
        assert!(x.events().is_empty());
    }

    #[test]
    fn paste_last_fires_once_per_press() {
        let mut x = Server::new(Some(Shortcut::ctrl_win()), Some(Shortcut::alt_shift_v()));
        x.key(LALT, true);
        x.key(LSHIFT, true);
        assert_eq!(x.key(KEY_V, true), Went::Kept);
        assert_eq!(x.key(KEY_V, true), Went::Kept);
        assert_eq!(x.key(KEY_V, false), Went::Kept);
        assert_eq!(x.events(), vec![E::PasteLast]);
        x.key(KEY_V, true);
        assert_eq!(x.events(), vec![E::PasteLast]);
    }

    #[test]
    fn paste_last_inside_the_dictation_chord() {
        let paste = Shortcut { ctrl: true, shift: false, alt: false, win: true, key: Some(KEY_V), key_label: None };
        let mut x = Server::new(Some(Shortcut::ctrl_win()), Some(paste));
        x.key(LCTRL, true);
        x.key(LWIN, true);
        assert_eq!(x.key(KEY_V, true), Went::Kept, "Talkr's own shortcut is not replayed past its grab");
        assert_eq!(x.events(), vec![E::DictateDown, E::Interrupted, E::PasteLast]);
    }

    #[test]
    fn capture_reports_the_whole_combination() {
        let mut c = Capture::default();
        assert_eq!(c.key(LCTRL, true), None);
        assert_eq!(c.key(LALT, true), None);
        assert_eq!(c.key(SPACE, true), None);
        assert_eq!(c.key(SPACE, true), None);
        assert_eq!(c.key(SPACE, false), None);
        assert_eq!(c.key(LALT, false), None, "reported once everything is up");
        match c.key(LCTRL, false) {
            Some(E::Captured(s)) => {
                assert!(s.ctrl && s.alt && !s.shift && !s.win);
                assert_eq!(s.key, Some(SPACE));
                assert_eq!(s.key_label.as_deref(), Some("Space"));
            }
            other => panic!("{other:?}"),
        }
        let mut c = Capture::default();
        c.key(LCTRL, true);
        c.key(LWIN, true);
        c.key(LWIN, false);
        assert_eq!(c.key(LCTRL, false), Some(E::Captured(Shortcut::ctrl_win())));
    }

    #[test]
    fn capture_cancels_on_escape_and_ignores_earlier_keys() {
        let mut c = Capture::default();
        assert_eq!(c.key(VK_ESCAPE, true), Some(E::CaptureCancelled));
        let mut c = Capture::default();
        // Released after recording began, pressed before: not part of it.
        assert_eq!(c.key(LCTRL, false), None);
        c.key(LSHIFT, true);
        c.key(VK_ESCAPE, true);
        c.key(VK_ESCAPE, false);
        match c.key(LSHIFT, false) {
            Some(E::Captured(s)) => assert!(s.shift && s.key == Some(VK_ESCAPE)),
            other => panic!("{other:?}"),
        }
    }
}
