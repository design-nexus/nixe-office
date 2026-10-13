//! As you type: straight quotes become curly ones, and a few symbols are made
//! from what's typed: -- → —, ... → …, (c) → ©, (r) → ®, (tm) → ™, -> → →.

use super::Writer;
use gtk::prelude::*;

/// What a typed character, with the text before it on its line, turns into:
/// (how many characters before it go too, the replacement). None leaves it.
pub fn correction(before: &str, typed: char) -> Option<(usize, &'static str)> {
    let prev = before.chars().next_back();
    // A quote opens after a space, an opening bracket, a dash or the line's start.
    let opens = prev.is_none_or(|p| p.is_whitespace() || "([{\u{2014}\u{2013}-/\u{201c}\u{2018}".contains(p));
    let lower = before.to_lowercase();
    match typed {
        '"' => Some((0, if opens { "\u{201c}" } else { "\u{201d}" })),
        '\'' => Some((0, if opens { "\u{2018}" } else { "\u{2019}" })),
        '-' if prev == Some('-') => Some((1, "\u{2014}")),
        '.' if before.ends_with("..") => Some((2, "\u{2026}")),
        '>' if prev == Some('-') => Some((1, "\u{2192}")),
        ')' if lower.ends_with("(c") => Some((2, "\u{a9}")),
        ')' if lower.ends_with("(r") => Some((2, "\u{ae}")),
        ')' if lower.ends_with("(tm") => Some((3, "\u{2122}")),
        _ => None,
    }
}

impl Writer {
    /// Correct the character just typed at offset `at`, when there's something to correct.
    pub(super) fn autocorrect(&self, at: i32) {
        if self.correcting.get() || !crate::prefs::get().autocorrect {
            return;
        }
        let b = self.tb();
        let it = b.iter_at_offset(at);
        let typed = it.char();
        let mut line_start = it;
        line_start.set_line_offset(0);
        let before = b.text(&line_start, &it, false).to_string();
        let Some((back, with)) = correction(&before, typed) else { return };
        // While tracking changes, only the typed character itself is replaced, so
        // no earlier text goes without a trace.
        if back > 0 && self.tracking.get() {
            return;
        }
        self.correcting.set(true);
        let mut s = b.iter_at_offset(at - back as i32);
        let mut e = b.iter_at_offset(at + 1);
        b.delete(&mut s, &mut e);
        let mut s = b.iter_at_offset(at - back as i32);
        b.insert(&mut s, with);
        self.correcting.set(false);
    }
}

#[cfg(test)]
mod tests {
    use super::correction;

    #[test]
    fn quotes_and_symbols() {
        assert_eq!(correction("", '"'), Some((0, "\u{201c}")));
        assert_eq!(correction("He said \u{201c}hi", '"'), Some((0, "\u{201d}")));
        assert_eq!(correction("don", '\''), Some((0, "\u{2019}")));
        assert_eq!(correction("a (", '\''), Some((0, "\u{2018}")));
        assert_eq!(correction("well-", '-'), Some((1, "\u{2014}")));
        assert_eq!(correction("and so..", '.'), Some((2, "\u{2026}")));
        assert_eq!(correction("2026 (C", ')'), Some((2, "\u{a9}")));
        assert_eq!(correction("Nexus(tm", ')'), Some((3, "\u{2122}")));
        assert_eq!(correction("a -", '>'), Some((1, "\u{2192}")));
        assert_eq!(correction("x", '-'), None);
        assert_eq!(correction("(a", ')'), None);
    }
}
