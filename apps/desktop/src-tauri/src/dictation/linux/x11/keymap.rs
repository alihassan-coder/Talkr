//! The server's keyboard mapping: which keycodes carry which keysyms, and which modifier bits
//! Alt, Super and the lock keys use (they differ between setups).

use x11rb::connection::Connection;
use x11rb::protocol::xproto::ConnectionExt as _;
use super::chord::{ALT, CTRL, SHIFT, WIN};
use super::keys::{self, Keysym, NO_SYMBOL, VK_UNKNOWN};

pub const SHIFT_MASK: u16 = 1;
pub const LOCK_MASK: u16 = 1 << 1;
pub const CONTROL_MASK: u16 = 1 << 2;
const MOD1_MASK: u16 = 1 << 3;
const MOD4_MASK: u16 = 1 << 6;

#[derive(Debug, Clone)]
pub struct Keymap {
    min: u8,
    per: usize,
    /// `per` keysyms for every keycode from `min`.
    syms: Vec<Keysym>,
    pub alt: u16,
    pub win: u16,
    pub num_lock: u16,
    pub scroll_lock: u16,
}

impl Keymap {
    pub fn load(conn: &impl Connection) -> Result<Self, String> {
        let setup = conn.setup();
        let (min, max) = (setup.min_keycode, setup.max_keycode);
        let mapping = conn
            .get_keyboard_mapping(min, max - min + 1)
            .map_err(|e| e.to_string())?
            .reply()
            .map_err(|e| e.to_string())?;
        let modifiers = conn.get_modifier_mapping().map_err(|e| e.to_string())?.reply().map_err(|e| e.to_string())?;
        Ok(Self::new(min, mapping.keysyms_per_keycode as usize, mapping.keysyms, &modifiers.keycodes))
    }

    /// `modifier_map` is the server's: eight rows (Shift, Lock, Control, Mod1-Mod5) of keycodes.
    pub fn new(min: u8, per: usize, syms: Vec<Keysym>, modifier_map: &[u8]) -> Self {
        let mut map = Self { min, per: per.max(1), syms, alt: 0, win: 0, num_lock: 0, scroll_lock: 0 };
        let row = modifier_map.len() / 8;
        if row > 0 {
            for (index, keycodes) in modifier_map.chunks(row).enumerate().skip(3) {
                let bit = 1u16 << index;
                for &kc in keycodes.iter().filter(|&&kc| kc != 0) {
                    for ks in map.keysyms(kc).to_vec() {
                        match ks {
                            keys::XK_ALT_L | keys::XK_ALT_R | keys::XK_META_L | keys::XK_META_R => map.alt |= bit,
                            keys::XK_SUPER_L | keys::XK_SUPER_R => map.win |= bit,
                            keys::XK_NUM_LOCK => map.num_lock |= bit,
                            keys::XK_SCROLL_LOCK => map.scroll_lock |= bit,
                            _ => {}
                        }
                    }
                }
            }
        }
        // The usual places, when the map does not say.
        if map.alt == 0 {
            map.alt = MOD1_MASK;
        }
        if map.win == 0 {
            map.win = MOD4_MASK;
        }
        map
    }

    pub fn keysyms(&self, keycode: u8) -> &[Keysym] {
        let Some(index) = (keycode as usize).checked_sub(self.min as usize) else { return &[] };
        self.syms.get(index * self.per..(index + 1) * self.per).unwrap_or(&[])
    }

    fn keycodes(&self) -> impl Iterator<Item = u8> + '_ {
        let count = self.syms.len() / self.per;
        (0..count).filter_map(move |i| u8::try_from(self.min as usize + i).ok())
    }

    /// The virtual-key code of a key, from its first two levels (`[KP_Home, KP_7]` is Num 7,
    /// `[1, exclam]` is 1). [`VK_UNKNOWN`] when it has none.
    pub fn vk(&self, keycode: u8) -> u16 {
        self.keysyms(keycode).iter().take(2).find_map(|&ks| keys::keysym_to_vk(ks)).unwrap_or(VK_UNKNOWN)
    }

    /// Every key that has this virtual-key code.
    pub fn keycodes_for_vk(&self, vk: u16) -> Vec<u8> {
        self.keycodes().filter(|&kc| self.vk(kc) == vk).collect()
    }

    /// A key with `ks` on its first level (no Shift needed).
    pub fn keycode_for(&self, ks: Keysym) -> Option<u8> {
        self.keycodes().find(|&kc| self.keysyms(kc).first() == Some(&ks))
    }

    /// Keys with no symbols at all, which can be borrowed to type any character.
    pub fn spare_keycodes(&self) -> Vec<u8> {
        self.keycodes().filter(|&kc| self.keysyms(kc).iter().all(|&ks| ks == NO_SYMBOL)).collect()
    }

    pub fn keysyms_per_keycode(&self) -> usize {
        self.per
    }

    /// Ctrl/Shift/Alt/Super bits held in an X event state or pointer mask.
    pub fn mods_of(&self, state: u16) -> u8 {
        let mut mods = 0;
        for (mask, bit) in [(CONTROL_MASK, CTRL), (SHIFT_MASK, SHIFT), (self.alt, ALT), (self.win, WIN)] {
            if state & mask != 0 {
                mods |= bit;
            }
        }
        mods
    }

    /// The X modifier mask for Ctrl/Shift/Alt/Super bits.
    pub fn mask_of(&self, mods: u8) -> u16 {
        let mut mask = 0;
        for (bit, m) in [(CTRL, CONTROL_MASK), (SHIFT, SHIFT_MASK), (ALT, self.alt), (WIN, self.win)] {
            if mods & bit != 0 {
                mask |= m;
            }
        }
        mask
    }

    /// Every combination of Caps Lock, Num Lock and Scroll Lock: a grab only matches the exact
    /// modifier state, and a lit lock key is part of it.
    pub fn lock_masks(&self) -> Vec<u16> {
        let locks: Vec<u16> = [LOCK_MASK, self.num_lock, self.scroll_lock]
            .into_iter()
            .filter(|&m| m != 0 && m & self.mask_of(CTRL | SHIFT | ALT | WIN) == 0)
            .collect();
        let mut masks: Vec<u16> = (0..1u32 << locks.len())
            .map(|set| locks.iter().enumerate().filter(|(i, _)| set & (1 << i) != 0).fold(0, |m, (_, &l)| m | l))
            .collect();
        masks.sort_unstable();
        masks.dedup();
        masks
    }
}

#[cfg(test)]
pub mod tests {
    use super::*;

    /// A small keymap shaped like a PC with a US layout: keycodes from 8, two levels.
    pub fn us() -> Keymap {
        let mut syms = vec![NO_SYMBOL; 2 * (140 - 8)];
        let mut set = |kc: usize, a: Keysym, b: Keysym| {
            syms[(kc - 8) * 2] = a;
            syms[(kc - 8) * 2 + 1] = b;
        };
        set(9, keys::XK_ESCAPE, 0);
        set(10, 0x31, 0x21); // 1 !
        set(38, 0x61, 0x41); // a A
        set(55, 0x76, 0x56); // v V
        set(65, 0x20, 0); // space
        set(37, keys::XK_CONTROL_L, 0);
        set(105, keys::XK_CONTROL_R, 0);
        set(50, keys::XK_SHIFT_L, 0);
        set(64, keys::XK_ALT_L, keys::XK_META_L);
        set(133, keys::XK_SUPER_L, 0);
        set(134, keys::XK_SUPER_R, 0);
        set(77, keys::XK_NUM_LOCK, 0);
        set(66, keys::XK_CAPS_LOCK, 0);
        set(79, 0xff95, 0xffb7); // KP_Home KP_7
        let mut modmap = vec![0u8; 8 * 2];
        modmap[0] = 50; // Shift
        modmap[2] = 66; // Lock
        modmap[4] = 37; // Control
        modmap[5] = 105;
        modmap[6] = 64; // Mod1
        modmap[8] = 77; // Mod2
        modmap[12] = 133; // Mod4
        modmap[13] = 134;
        Keymap::new(8, 2, syms, &modmap)
    }

    #[test]
    fn modifier_bits_come_from_the_map() {
        let map = us();
        assert_eq!((map.alt, map.win, map.num_lock, map.scroll_lock), (1 << 3, 1 << 6, 1 << 4, 0));
        assert_eq!(map.mods_of(CONTROL_MASK | (1 << 6) | LOCK_MASK | (1 << 4)), CTRL | WIN);
        assert_eq!(map.mask_of(ALT | SHIFT), (1 << 3) | SHIFT_MASK);
        // Super on Mod3 in some setups.
        let mut modmap = vec![0u8; 16];
        modmap[10] = 133;
        let odd = Keymap::new(8, 2, us().syms, &modmap);
        assert_eq!(odd.win, 1 << 5);
        assert_eq!(odd.num_lock, 0);
    }

    #[test]
    fn grabs_cover_every_lock_combination() {
        let map = us();
        assert_eq!(map.lock_masks(), vec![0, LOCK_MASK, 1 << 4, LOCK_MASK | (1 << 4)]);
        let mut with_scroll = map.clone();
        with_scroll.scroll_lock = 1 << 7;
        assert_eq!(with_scroll.lock_masks().len(), 8);
        // A lock that shares Super's bit would make Super optional: left out.
        let mut clash = map;
        clash.num_lock = clash.win;
        assert_eq!(clash.lock_masks(), vec![0, LOCK_MASK]);
    }

    #[test]
    fn keys_are_found_by_code_and_symbol() {
        let map = us();
        assert_eq!(map.vk(38), 0x41);
        assert_eq!(map.vk(10), 0x31);
        assert_eq!(map.vk(79), 0x67, "keypad 7 from its second level");
        assert_eq!(map.vk(200), VK_UNKNOWN);
        assert_eq!(map.keycodes_for_vk(keys::VK_LWIN), vec![133]);
        assert_eq!(map.keycodes_for_vk(keys::VK_LCONTROL), vec![37]);
        assert_eq!(map.keycode_for(keys::XK_V), Some(55));
        assert_eq!(map.keycode_for(0x56), None, "V is on the second level");
        assert!(map.spare_keycodes().contains(&8));
        assert!(!map.spare_keycodes().contains(&38));
        assert!(map.keysyms(7).is_empty() && map.keysyms(250).is_empty());
    }
}
