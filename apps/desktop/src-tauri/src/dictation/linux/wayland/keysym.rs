//! Text as key presses for the RemoteDesktop portal, which takes X keysyms: Latin-1 characters
//! are their own keysym, every other character is 0x01000000 + its code point, and a line break
//! is Return. The compositor finds the key (and Shift or AltGr) on the active layout.

pub const RETURN: u32 = 0xff0d;
pub const TAB: u32 = 0xff09;
pub const SHIFT_L: u32 = 0xffe1;
pub const INSERT: u32 = 0xff63;

/// One key going down (`true`) or up.
pub type Stroke = (u32, bool);

/// The keysym that types `c`, if it can be typed.
pub fn for_char(c: char) -> Option<u32> {
    match c {
        '\n' => Some(RETURN),
        '\t' => Some(TAB),
        c if c.is_control() => None,
        ' '..='~' | '\u{a0}'..='\u{ff}' => Some(c as u32),
        c => Some(0x0100_0000 + c as u32),
    }
}

/// The key presses that type `text`. A Windows line break ("\r\n") is one Return; other control
/// characters are left out.
pub fn strokes(text: &str) -> Vec<Stroke> {
    let normalized = text.replace("\r\n", "\n").replace('\r', "\n");
    let mut out = Vec::with_capacity(normalized.len() * 2);
    for keysym in normalized.chars().filter_map(for_char) {
        out.push((keysym, true));
        out.push((keysym, false));
    }
    out
}

/// Shift + Insert: paste in GTK, Qt, Chromium and Electron apps, LibreOffice, and in terminals,
/// where Ctrl + V is a control key. (Terminals that paste the primary selection with it get the
/// text there too, when the compositor lets Talkr set it.)
pub fn paste() -> Vec<Stroke> {
    vec![(SHIFT_L, true), (INSERT, true), (INSERT, false), (SHIFT_L, false)]
}

/// The keys still held after `sent` of `strokes` went through: released after a failure, so
/// nothing stays stuck down.
pub fn held_after(strokes: &[Stroke], sent: usize) -> Vec<u32> {
    let mut held: Vec<u32> = Vec::new();
    for &(keysym, down) in strokes.iter().take(sent) {
        if down {
            held.push(keysym);
        } else {
            held.retain(|k| *k != keysym);
        }
    }
    held
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn latin1_is_its_own_keysym() {
        assert_eq!(for_char('a'), Some(0x61));
        assert_eq!(for_char('Z'), Some(0x5a));
        assert_eq!(for_char(' '), Some(0x20));
        assert_eq!(for_char('~'), Some(0x7e));
        assert_eq!(for_char('é'), Some(0xe9));
        assert_eq!(for_char('ÿ'), Some(0xff));
        assert_eq!(for_char('ß'), Some(0xdf));
        assert_eq!(for_char('\u{a0}'), Some(0xa0));
    }

    #[test]
    fn everything_else_is_unicode() {
        assert_eq!(for_char('€'), Some(0x0100_20ac));
        assert_eq!(for_char('ж'), Some(0x0100_0436));
        assert_eq!(for_char('中'), Some(0x0100_4e2d));
        assert_eq!(for_char('😀'), Some(0x0101_f600));
    }

    #[test]
    fn line_breaks_and_controls() {
        assert_eq!(for_char('\n'), Some(RETURN));
        assert_eq!(for_char('\t'), Some(TAB));
        assert_eq!(for_char('\u{7f}'), None);
        assert_eq!(for_char('\u{1b}'), None);
        assert_eq!(strokes("a\r\nb"), vec![(0x61, true), (0x61, false), (RETURN, true), (RETURN, false), (0x62, true), (0x62, false)]);
        assert_eq!(strokes("a\rb").len(), 6);
        assert_eq!(strokes("x\u{7}"), vec![(0x78, true), (0x78, false)]);
        assert!(strokes("").is_empty());
    }

    #[test]
    fn each_key_goes_down_then_up() {
        let s = strokes("Hi ж");
        assert_eq!(s.len(), 8);
        for pair in s.chunks(2) {
            assert_eq!(pair[0].0, pair[1].0);
            assert!(pair[0].1 && !pair[1].1);
        }
    }

    #[test]
    fn nothing_stays_held() {
        let p = paste();
        assert_eq!(held_after(&p, 0), Vec::<u32>::new());
        assert_eq!(held_after(&p, 1), vec![SHIFT_L]);
        assert_eq!(held_after(&p, 2), vec![SHIFT_L, INSERT]);
        assert_eq!(held_after(&p, 3), vec![SHIFT_L]);
        assert!(held_after(&p, 4).is_empty());
        assert!(held_after(&p, 99).is_empty());
    }
}
