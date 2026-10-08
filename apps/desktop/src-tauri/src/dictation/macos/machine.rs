//! What the shortcut listener makes of each key event, as a pure state machine over Windows
//! virtual-key codes (see `keymap`). It has the same semantics as the Windows hook
//! (`win/hook.rs`), so a shortcut behaves alike on both systems, and it is tested here without a
//! keyboard.

use super::keymap::{self, VK_ESCAPE};
use crate::dictation::settings::Shortcut;
use crate::dictation::HotkeyEvent;

pub const CTRL: u8 = 1;
pub const SHIFT: u8 = 2;
pub const ALT: u8 = 4;
pub const WIN: u8 = 8;

const ENABLED_BIT: u64 = 1 << 32;

/// A shortcut as the listener matches it: modifier bits and at most one other key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Chord {
    pub mods: u8,
    pub key: Option<u16>,
}

impl Chord {
    /// Pack a shortcut into one word, so it can sit in an atomic the listener thread reads.
    /// 0 means "off".
    pub fn encode(shortcut: Option<&Shortcut>) -> u64 {
        let Some(s) = shortcut else { return 0 };
        let mut mods = 0u8;
        for (on, bit) in [(s.ctrl, CTRL), (s.shift, SHIFT), (s.alt, ALT), (s.win, WIN)] {
            if on {
                mods |= bit;
            }
        }
        ENABLED_BIT | (mods as u64) << 16 | s.key.unwrap_or(0) as u64
    }

    pub fn decode(v: u64) -> Option<Chord> {
        (v & ENABLED_BIT != 0).then(|| {
            let key = (v & 0xFFFF) as u16;
            Chord { mods: ((v >> 16) & 0xFF) as u8, key: (key != 0).then_some(key) }
        })
    }
}

/// What the listener is told to look for, read fresh for every event.
#[derive(Debug, Clone, Copy, Default)]
pub struct Config {
    pub dictate: Option<Chord>,
    pub paste_last: Option<Chord>,
    /// A dictation is recording: Escape cancels it.
    pub recording: bool,
    /// The settings page is recording a new shortcut.
    pub capturing: bool,
}

/// The result of one key event.
#[derive(Debug, Default, PartialEq)]
pub struct Step {
    /// Keep the event from the focused app.
    pub swallow: bool,
    pub events: Vec<HotkeyEvent>,
    /// Shortcut capture finished (with a shortcut or cancelled): stop capturing.
    pub capture_ended: bool,
}

pub fn modifier_bit(vk: u16) -> u8 {
    match vk {
        0x11 | keymap::VK_LCONTROL | keymap::VK_RCONTROL => CTRL,
        0x10 | keymap::VK_LSHIFT | keymap::VK_RSHIFT => SHIFT,
        0x12 | keymap::VK_LMENU | keymap::VK_RMENU => ALT,
        keymap::VK_LWIN | keymap::VK_RWIN => WIN,
        _ => 0,
    }
}

fn is_modifier(vk: u16) -> bool {
    modifier_bit(vk) != 0
}

#[derive(Debug, Default)]
pub struct Machine {
    /// Keys held right now, as the listener saw them.
    down: Vec<u16>,
    /// Keys whose press was kept from the app: their release must not reach it either, or the
    /// app would see a key go up that never went down.
    swallowed: Vec<u16>,
    dictate_active: bool,
    /// A modifier-only dictation shortcut met another key (Ctrl + Cmd + Space): ignore it until
    /// its modifiers are released.
    dictate_blocked: bool,
    paste_active: bool,
    capture: Option<Chord>,
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
    /// Nothing held and nothing in progress.
    #[cfg(test)]
    fn is_idle(&self) -> bool {
        self.down.is_empty() && !self.dictate_active && self.capture.is_none()
    }

    fn swallow(&mut self, vk: u16) {
        if !self.swallowed.contains(&vk) {
            self.swallowed.push(vk);
        }
    }

    fn mods(&self) -> u8 {
        self.down.iter().fold(0, |m, &vk| m | modifier_bit(vk))
    }

    /// One key event. `physically_down` says whether a key is held right now (to forget keys
    /// whose release the listener never saw); `label` names a key for a captured shortcut.
    pub fn handle(
        &mut self,
        config: &Config,
        vk: u16,
        down: bool,
        physically_down: &dyn Fn(u16) -> bool,
        label: &dyn Fn(u16) -> String,
    ) -> Step {
        let mut step = Step::default();
        let repeat = down && self.down.contains(&vk);
        if down && repeat && self.swallowed.contains(&vk) {
            // Auto-repeat of a key whose press was kept from the app: keep the repeats too, even
            // after the shortcut ended (a modifier let go first), or the app would type them.
            step.swallow = true;
            return step;
        }
        if down && !repeat {
            // A key released while the listener could not see it (secure input, a stalled tap):
            // drop keys the system says are up, so they can neither break shortcuts nor have a
            // later press swallowed.
            self.down.retain(|&k| k == vk || physically_down(k));
            self.swallowed.retain(|&k| k != vk && physically_down(k));
            self.down.push(vk);
        }
        if !down {
            remove(&mut self.down, vk);
        }
        let was_swallowed = !down && remove(&mut self.swallowed, vk);

        if config.capturing {
            step.swallow = self.capture(&mut step, vk, down, repeat, label) || was_swallowed;
            return step;
        }
        if self.capture.take().is_some() {
            // Capture was ended from outside mid-way.
            self.swallowed.clear();
        }

        if vk == VK_ESCAPE && config.recording {
            if down {
                if !repeat {
                    step.events.push(HotkeyEvent::Cancel);
                }
                self.swallow(vk);
                step.swallow = true;
            } else {
                step.swallow = was_swallowed;
            }
            return step;
        }

        let mods = self.mods();
        let mut swallow = was_swallowed;
        if let Some(chord) = config.dictate {
            swallow |= self.dictate(&mut step, chord, vk, down, repeat, mods);
        }
        if let Some(chord) = config.paste_last {
            swallow |= self.paste_last(&mut step, chord, vk, down, repeat, mods);
        }
        step.swallow = swallow;
        step
    }

    fn dictate(&mut self, step: &mut Step, chord: Chord, vk: u16, down: bool, repeat: bool, mods: u8) -> bool {
        match chord.key {
            Some(key) => {
                if vk == key {
                    if down {
                        if self.dictate_active && repeat {
                            return true; // auto-repeat while held
                        }
                        // A fresh press while "active" means the release was never seen.
                        self.dictate_active = false;
                        if !repeat && mods == chord.mods {
                            self.dictate_active = true;
                            self.swallow(vk);
                            step.events.push(HotkeyEvent::DictateDown);
                            return true;
                        }
                    } else if self.dictate_active {
                        self.dictate_active = false;
                        step.events.push(HotkeyEvent::DictateUp);
                    }
                } else if self.dictate_active && !down && is_modifier(vk) && mods & chord.mods != chord.mods {
                    // A modifier of the shortcut let go first.
                    self.dictate_active = false;
                    step.events.push(HotkeyEvent::DictateUp);
                }
                false
            }
            None => {
                if is_modifier(vk) {
                    if down && !repeat {
                        if self.dictate_active {
                            if mods != chord.mods {
                                // Another modifier joined: that is a different shortcut.
                                self.dictate_active = false;
                                self.dictate_blocked = true;
                                step.events.push(HotkeyEvent::Interrupted);
                            }
                        } else if !self.dictate_blocked && mods == chord.mods && self.down.iter().all(|&k| is_modifier(k)) {
                            self.dictate_active = true;
                            step.events.push(HotkeyEvent::DictateDown);
                        }
                    } else if !down {
                        if self.dictate_active && mods & chord.mods != chord.mods {
                            self.dictate_active = false;
                            step.events.push(HotkeyEvent::DictateUp);
                        }
                        if mods & chord.mods == 0 {
                            self.dictate_blocked = false;
                        }
                    }
                } else if down && !repeat && self.dictate_active {
                    // Ctrl + Cmd + Space and friends: the system's shortcut, not dictation.
                    self.dictate_active = false;
                    self.dictate_blocked = true;
                    step.events.push(HotkeyEvent::Interrupted);
                }
                false
            }
        }
    }

    fn paste_last(&mut self, step: &mut Step, chord: Chord, vk: u16, down: bool, repeat: bool, mods: u8) -> bool {
        match chord.key {
            Some(key) if vk == key => {
                if down && !repeat && mods == chord.mods {
                    self.paste_active = true;
                    self.swallow(vk);
                    step.events.push(HotkeyEvent::PasteLast);
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
                if is_modifier(vk) && down && !repeat && !self.paste_active && mods == chord.mods {
                    self.paste_active = true;
                    step.events.push(HotkeyEvent::PasteLast);
                } else if !down && mods & chord.mods != chord.mods {
                    self.paste_active = false;
                }
                false
            }
        }
    }

    /// Recording a new shortcut: every key is kept from the apps, and the combination is reported
    /// once all keys are up. Escape alone cancels.
    fn capture(&mut self, step: &mut Step, vk: u16, down: bool, repeat: bool, label: &dyn Fn(u16) -> String) -> bool {
        if down {
            if repeat {
                return true;
            }
            if vk == VK_ESCAPE && self.capture.is_none() {
                step.capture_ended = true;
                self.swallow(vk);
                step.events.push(HotkeyEvent::CaptureCancelled);
                return true;
            }
            let chord = self.capture.get_or_insert(Chord { mods: 0, key: None });
            chord.mods |= modifier_bit(vk);
            if !is_modifier(vk) {
                chord.key = Some(vk);
            }
            self.swallow(vk);
            return true;
        }
        if self.down.is_empty() {
            if let Some(chord) = self.capture.take() {
                step.capture_ended = true;
                step.events.push(HotkeyEvent::Captured(Shortcut {
                    ctrl: chord.mods & CTRL != 0,
                    shift: chord.mods & SHIFT != 0,
                    alt: chord.mods & ALT != 0,
                    win: chord.mods & WIN != 0,
                    key: chord.key,
                    key_label: chord.key.map(label),
                }));
            }
        }
        // Releases of keys pressed before capture began belong to the app.
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dictation::HotkeyEvent as E;
    use keymap::{vk_from_mac, VK_LCONTROL as LCTRL, VK_LMENU, VK_LSHIFT, VK_LWIN as LCMD};

    const SPACE: u16 = 0x20;
    const LEFT: u16 = 0x25;
    const V: u16 = 0x56;

    /// Key events through the machine, as the listener would feed them.
    struct Harness {
        machine: Machine,
        config: Config,
        events: Vec<HotkeyEvent>,
    }

    impl Harness {
        fn new(dictate: Option<Shortcut>, paste: Option<Shortcut>) -> Self {
            let config = Config {
                dictate: Chord::decode(Chord::encode(dictate.as_ref())),
                paste_last: Chord::decode(Chord::encode(paste.as_ref())),
                ..Config::default()
            };
            Self { machine: Machine::default(), config, events: Vec::new() }
        }

        fn key(&mut self, vk: u16, down: bool) -> bool {
            let held = self.machine.down.clone();
            let step = self.machine.handle(&self.config, vk, down, &move |k| held.contains(&k), &keymap::key_label);
            if step.capture_ended {
                self.config.capturing = false;
            }
            self.events.extend(step.events);
            step.swallow
        }

        fn events(&mut self) -> Vec<HotkeyEvent> {
            std::mem::take(&mut self.events)
        }
    }

    fn ctrl_space() -> Shortcut {
        Shortcut { ctrl: true, shift: false, alt: false, win: false, key: Some(SPACE), key_label: None }
    }

    #[test]
    fn chords_pack_and_unpack() {
        assert_eq!(Chord::encode(None), 0);
        assert_eq!(Chord::decode(0), None);
        assert_eq!(Chord::decode(Chord::encode(Some(&Shortcut::ctrl_win()))), Some(Chord { mods: CTRL | WIN, key: None }));
        assert_eq!(
            Chord::decode(Chord::encode(Some(&Shortcut::alt_shift_v()))),
            Some(Chord { mods: ALT | SHIFT, key: Some(V) })
        );
    }

    #[test]
    fn modifier_chord_reports_press_and_release() {
        let mut h = Harness::new(Some(Shortcut::ctrl_win()), None);
        assert!(!h.key(LCTRL, true));
        assert!(!h.key(LCMD, true));
        assert_eq!(h.events(), vec![E::DictateDown]);
        // A second FlagsChanged for a key already down changes nothing.
        h.key(LCMD, true);
        assert!(h.events().is_empty());
        assert!(!h.key(LCMD, false));
        assert_eq!(h.events(), vec![E::DictateUp]);
        h.key(LCTRL, false);
        assert!(h.events().is_empty());
    }

    #[test]
    fn either_side_of_a_modifier_works() {
        let mut h = Harness::new(Some(Shortcut::ctrl_win()), None);
        h.key(keymap::VK_RCONTROL, true);
        h.key(keymap::VK_RWIN, true);
        assert_eq!(h.events(), vec![E::DictateDown]);
        h.key(keymap::VK_RCONTROL, false);
        assert_eq!(h.events(), vec![E::DictateUp]);
    }

    #[test]
    fn system_shortcuts_interrupt_and_block_until_released() {
        let mut h = Harness::new(Some(Shortcut::ctrl_win()), None);
        h.key(LCTRL, true);
        h.key(LCMD, true);
        assert!(!h.key(LEFT, true), "the arrow reaches macOS");
        assert_eq!(h.events(), vec![E::DictateDown, E::Interrupted]);
        h.key(LEFT, false);
        h.key(LEFT, true);
        h.key(LEFT, false);
        h.key(LCMD, false);
        h.key(LCTRL, false);
        assert!(h.events().is_empty());
        h.key(LCTRL, true);
        h.key(LCMD, true);
        assert_eq!(h.events(), vec![E::DictateDown]);
    }

    #[test]
    fn another_modifier_joining_interrupts() {
        let mut h = Harness::new(Some(Shortcut::ctrl_win()), None);
        h.key(LCTRL, true);
        h.key(LCMD, true);
        h.key(VK_LSHIFT, true);
        assert_eq!(h.events(), vec![E::DictateDown, E::Interrupted]);
        h.key(VK_LSHIFT, false);
        assert!(h.events().is_empty(), "blocked until the shortcut's modifiers are released");
    }

    #[test]
    fn chord_only_fires_on_exact_modifiers() {
        let mut h = Harness::new(Some(Shortcut::ctrl_win()), None);
        h.key(VK_LSHIFT, true);
        h.key(LCTRL, true);
        h.key(LCMD, true);
        assert!(h.events().is_empty(), "Ctrl + Shift + Cmd is another shortcut");
        let mut h = Harness::new(Some(Shortcut::ctrl_win()), None);
        h.key(0x41, true);
        h.key(LCTRL, true);
        h.key(LCMD, true);
        assert!(h.events().is_empty(), "a letter held first: typing, not dictation");
    }

    #[test]
    fn key_chord_swallows_its_key() {
        let mut h = Harness::new(Some(ctrl_space()), None);
        assert!(!h.key(LCTRL, true));
        assert!(h.key(SPACE, true), "kept from the app");
        assert!(h.key(SPACE, true), "repeats too");
        assert!(h.key(SPACE, false), "and the release");
        assert!(!h.key(LCTRL, false));
        assert_eq!(h.events(), vec![E::DictateDown, E::DictateUp]);
        assert!(!h.key(SPACE, true), "Space alone is typing");
        assert!(!h.key(SPACE, false));
        assert!(h.events().is_empty());
    }

    #[test]
    fn a_held_key_stays_swallowed_after_its_modifier_lets_go() {
        let mut h = Harness::new(Some(ctrl_space()), None);
        h.key(LCTRL, true);
        assert!(h.key(SPACE, true));
        h.key(LCTRL, false);
        assert_eq!(h.events(), vec![E::DictateDown, E::DictateUp]);
        assert!(h.key(SPACE, true));
        assert!(h.key(SPACE, true));
        assert!(h.key(SPACE, false));
        assert!(!h.key(SPACE, true));
        assert!(!h.key(SPACE, false));
    }

    #[test]
    fn standalone_function_key_works_alone() {
        let f13 = Shortcut { ctrl: false, shift: false, alt: false, win: false, key: Some(0x7C), key_label: None };
        let mut h = Harness::new(Some(f13), None);
        assert!(h.key(vk_from_mac(0x69), true));
        assert!(h.key(vk_from_mac(0x69), false));
        assert_eq!(h.events(), vec![E::DictateDown, E::DictateUp]);
    }

    #[test]
    fn holding_escape_does_not_swallow_later_escapes() {
        let mut h = Harness::new(Some(Shortcut::ctrl_win()), None);
        h.config.recording = true;
        for _ in 0..30 {
            assert!(h.key(VK_ESCAPE, true));
        }
        assert!(h.key(VK_ESCAPE, false));
        h.config.recording = false;
        assert!(!h.key(VK_ESCAPE, true));
        assert!(!h.key(VK_ESCAPE, false), "a later Escape release reaches the app");
        assert_eq!(h.events(), vec![E::Cancel]);
    }

    #[test]
    fn a_missed_release_does_not_swallow_the_next_press() {
        let mut h = Harness::new(Some(ctrl_space()), None);
        h.key(LCTRL, true);
        assert!(h.key(SPACE, true));
        // Both releases happen while secure input hides the keyboard: never seen.
        h.machine.down.clear();
        h.events();
        assert!(!h.key(SPACE, true), "a fresh Space press is typing");
    }

    #[test]
    fn escape_cancels_only_while_recording() {
        let mut h = Harness::new(Some(Shortcut::ctrl_win()), None);
        assert!(!h.key(VK_ESCAPE, true));
        h.key(VK_ESCAPE, false);
        h.config.recording = true;
        assert!(h.key(VK_ESCAPE, true));
        assert!(h.key(VK_ESCAPE, false));
        assert_eq!(h.events(), vec![E::Cancel]);
    }

    #[test]
    fn paste_last_fires_once_per_press() {
        let mut h = Harness::new(Some(Shortcut::ctrl_win()), Some(Shortcut::alt_shift_v()));
        h.key(VK_LMENU, true);
        h.key(VK_LSHIFT, true);
        assert!(h.key(V, true));
        assert!(h.key(V, true));
        assert!(h.key(V, false));
        assert_eq!(h.events(), vec![E::PasteLast]);
    }

    #[test]
    fn capture_reports_the_whole_combination() {
        let mut h = Harness::new(Some(Shortcut::ctrl_win()), None);
        h.config.capturing = true;
        assert!(h.key(LCTRL, true));
        assert!(h.key(VK_LMENU, true));
        assert!(h.key(SPACE, true));
        h.key(SPACE, false);
        h.key(VK_LMENU, false);
        assert!(h.events().is_empty(), "reported once everything is up");
        h.key(LCTRL, false);
        match h.events().as_slice() {
            [E::Captured(s)] => {
                assert!(s.ctrl && s.alt && !s.shift && !s.win);
                assert_eq!(s.key, Some(SPACE));
                assert_eq!(s.key_label.as_deref(), Some("Space"));
            }
            other => panic!("{other:?}"),
        }
        assert!(!h.config.capturing);
        h.config.capturing = true;
        assert!(h.key(VK_ESCAPE, true));
        assert_eq!(h.events(), vec![E::CaptureCancelled]);
        assert!(!h.config.capturing);
        assert!(h.key(VK_ESCAPE, false), "the Escape that cancelled stays with Talkr");
    }

    #[test]
    fn capture_of_modifiers_only() {
        let mut h = Harness::new(Some(Shortcut::ctrl_win()), None);
        h.config.capturing = true;
        h.key(VK_LMENU, true);
        h.key(LCMD, true);
        h.key(LCMD, false);
        h.key(VK_LMENU, false);
        match h.events().as_slice() {
            [E::Captured(s)] => {
                assert!(s.alt && s.win && !s.ctrl && !s.shift);
                assert_eq!(s.key, None);
                assert_eq!(s.key_label, None);
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn nothing_triggers_while_capturing() {
        let mut h = Harness::new(Some(Shortcut::ctrl_win()), None);
        h.config.capturing = true;
        h.key(LCTRL, true);
        h.key(LCMD, true);
        assert!(h.events().is_empty());
    }

    #[test]
    fn a_capture_ended_from_outside_starts_clean() {
        let mut h = Harness::new(Some(ctrl_space()), None);
        h.config.capturing = true;
        assert!(h.key(0x41, true));
        h.config.capturing = false;
        assert!(h.key(0x41, false), "its press was kept from the app, so its release is too");
        assert!(h.machine.is_idle());
        assert!(h.events().is_empty());
        h.key(LCTRL, true);
        assert!(h.key(SPACE, true));
        assert_eq!(h.events(), vec![E::DictateDown]);
    }
}
