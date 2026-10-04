//! The input text as a sequence of characters. Positions are character indices; every
//! character, including spaces and line breaks, is significant.
//!
//! Word and number runs are found once, up front, so `WORD` and `FLOAT` can match a whole
//! run in one step and never match half of one.

pub struct Input<'a> {
    pub text: &'a str,
    chars: Vec<char>,
    /// Byte offset of each character, plus `text.len()` at the end.
    offsets: Vec<u32>,
    /// At the start of a WORD run: the position where it ends. 0 elsewhere.
    word_end: Vec<u32>,
    /// At the start of a FLOAT run: the position where it ends. 0 elsewhere.
    number_end: Vec<u32>,
    /// The position where each line starts, ascending.
    line_starts: Vec<u32>,
}

/// Combining marks (accents written as separate code points) belong to the letter before them.
pub fn is_mark(c: char) -> bool {
    matches!(c as u32, 0x0300..=0x036F | 0x1AB0..=0x1AFF | 0x1DC0..=0x1DFF | 0x20D0..=0x20FF | 0xFE20..=0xFE2F)
        || matches!(c, '\u{200D}' | '\u{FE0F}')
}

pub fn is_punct(c: char) -> bool {
    !c.is_alphanumeric() && !c.is_whitespace()
}

/// An IPv6 address in RFC 4291 text form. An IPv4 tail follows the rules of `IPV4`.
fn is_ipv6(s: &[char]) -> bool {
    let s: String = s.iter().collect();
    // The number of 16-bit groups in a `:`-separated part, if it is well formed.
    let groups = |part: &str, v4_last: bool| -> Option<usize> {
        if part.is_empty() {
            return Some(0);
        }
        let items: Vec<&str> = part.split(':').collect();
        let mut n = 0;
        for (k, g) in items.iter().enumerate() {
            if v4_last && k == items.len() - 1 && g.contains('.') {
                let parts: Vec<&str> = g.split('.').collect();
                let ok = parts.len() == 4
                    && parts.iter().all(|p| {
                        (1..=3).contains(&p.len())
                            && p.bytes().all(|b| b.is_ascii_digit())
                            && p.parse::<u32>().is_ok_and(|v| v <= 255)
                    });
                if !ok {
                    return None;
                }
                n += 2;
            } else if (1..=4).contains(&g.len()) && g.bytes().all(|b| b.is_ascii_hexdigit()) {
                n += 1;
            } else {
                return None;
            }
        }
        Some(n)
    };
    match s.split_once("::") {
        Some((head, tail)) => {
            !tail.contains("::")
                && matches!((groups(head, false), groups(tail, true)), (Some(h), Some(t)) if h + t <= 7)
        }
        None => groups(&s, true) == Some(8),
    }
}

impl<'a> Input<'a> {
    pub fn new(text: &'a str) -> Input<'a> {
        let mut chars = Vec::with_capacity(text.len());
        let mut offsets = Vec::with_capacity(text.len() + 1);
        for (i, c) in text.char_indices() {
            chars.push(c);
            offsets.push(i as u32);
        }
        offsets.push(text.len() as u32);
        let n = chars.len();
        let mut word_end = vec![0; n];
        let mut number_end = vec![0; n];
        let mut i = 0;
        while i < n {
            let c = chars[i];
            if c.is_alphabetic() {
                let mut j = i + 1;
                while j < n && (chars[j].is_alphabetic() || is_mark(chars[j])) {
                    j += 1;
                }
                word_end[i] = j as u32;
                i = j;
            } else if c.is_ascii_digit() {
                let mut j = i + 1;
                while j < n && chars[j].is_ascii_digit() {
                    j += 1;
                }
                if j + 1 < n && chars[j] == '.' && chars[j + 1].is_ascii_digit() {
                    j += 1;
                    while j < n && chars[j].is_ascii_digit() {
                        j += 1;
                    }
                }
                number_end[i] = j as u32;
                i = j;
            } else {
                i += 1;
            }
        }
        let mut line_starts = vec![0];
        for (k, &c) in chars.iter().enumerate() {
            let breaks = c == '\n' || (c == '\r' && chars.get(k + 1) != Some(&'\n'));
            if breaks {
                line_starts.push(k as u32 + 1);
            }
        }
        Input { text, chars, offsets, word_end, number_end, line_starts }
    }

    /// Line and column of position `i`, both counted from 1 (columns in characters).
    pub fn row_col(&self, i: usize) -> (usize, usize) {
        let line = self.line_starts.partition_point(|&s| s as usize <= i);
        (line, i - self.line_starts[line - 1] as usize + 1)
    }

    /// Number of characters.
    pub fn len(&self) -> usize {
        self.chars.len()
    }

    pub fn is_empty(&self) -> bool {
        self.chars.is_empty()
    }

    pub fn char(&self, i: usize) -> Option<char> {
        self.chars.get(i).copied()
    }

    /// Byte offset of position `i` (`i` may be `len()`).
    pub fn byte_pos(&self, i: usize) -> usize {
        self.offsets[i] as usize
    }

    /// Byte range of positions `i..j`.
    pub fn byte_span(&self, i: usize, j: usize) -> (usize, usize) {
        (self.byte_pos(i), self.byte_pos(j))
    }

    /// Text of positions `i..j`.
    pub fn slice(&self, i: usize, j: usize) -> &'a str {
        &self.text[self.byte_pos(i)..self.byte_pos(j)]
    }

    /// End of the WORD run starting at `i` (letters and combining marks).
    pub fn word_at(&self, i: usize) -> Option<usize> {
        self.word_end.get(i).filter(|&&e| e > 0).map(|&e| e as usize)
    }

    /// End of the FLOAT run starting at `i` (`42`, `3.5`).
    pub fn number_at(&self, i: usize) -> Option<usize> {
        self.number_end.get(i).filter(|&&e| e > 0).map(|&e| e as usize)
    }

    /// End of a maximal run of `digit` characters starting at `i`: not preceded by one, and
    /// (with `alnum_after`) not followed by any letter or digit either.
    fn run_at(&self, i: usize, digit: impl Fn(char) -> bool, alnum_after: bool) -> Option<usize> {
        if !digit(self.char(i)?) || i.checked_sub(1).and_then(|p| self.char(p)).is_some_and(&digit) {
            return None;
        }
        let mut j = i + 1;
        while self.char(j).is_some_and(&digit) {
            j += 1;
        }
        let blocked = |c: char| if alnum_after { c.is_alphanumeric() } else { digit(c) };
        (!self.char(j).is_some_and(blocked)).then_some(j)
    }

    /// INT: a whole run of ASCII digits (`3.14` holds two: `3` and `14`).
    pub fn int_at(&self, i: usize) -> Option<usize> {
        self.run_at(i, |c| c.is_ascii_digit(), false)
    }

    /// HEX: a run of hexadecimal digits not followed by other letters or digits.
    pub fn hex_at(&self, i: usize) -> Option<usize> {
        self.run_at(i, |c| c.is_ascii_hexdigit(), true)
    }

    /// BIN: a run of `0`/`1` not followed by other letters or digits.
    pub fn bin_at(&self, i: usize) -> Option<usize> {
        self.run_at(i, |c| c == '0' || c == '1', true)
    }

    /// IPV4: four numbers from 0 to 255 separated by dots, not part of a longer number.
    pub fn ipv4_at(&self, i: usize) -> Option<usize> {
        let mut j = i;
        for part in 0..4 {
            if part > 0 {
                if self.char(j) != Some('.') {
                    return None;
                }
                j += 1;
            }
            let end = self.int_at(j)?;
            if end - j > 3 || self.slice(j, end).parse::<u32>().ok()? > 255 {
                return None;
            }
            j = end;
        }
        // Not a prefix of a longer dotted number (`1.2.3.4.5`).
        let continues = self.char(j) == Some('.') && self.char(j + 1).is_some_and(|c| c.is_ascii_digit());
        let preceded = i > 0 && self.char(i - 1) == Some('.');
        (!continues && !preceded).then_some(j)
    }

    /// IPV6: the longest IPv6 address in RFC 4291 text form at `i` (hex groups, at most one
    /// `::`, an optional dotted IPv4 tail), not part of a longer word, number or address.
    pub fn ipv6_at(&self, i: usize) -> Option<usize> {
        let boundary = |c: char| c.is_alphanumeric() || c == ':' || c == '.';
        if i.checked_sub(1).and_then(|p| self.char(p)).is_some_and(boundary) {
            return None;
        }
        // The longest address is 45 characters (`ffff:…:255.255.255.255`).
        let mut span = i;
        while span - i < 45 && self.char(span).is_some_and(|c| c.is_ascii_hexdigit() || c == ':' || c == '.') {
            span += 1;
        }
        (i + 2..=span).rev().find(|&end| {
            let cut = match self.char(end) {
                None => false,
                Some(c @ (':' | '.')) => {
                    self.char(end + 1).is_some_and(|d| d.is_ascii_hexdigit() || (c == ':' && d == ':'))
                }
                Some(c) => c.is_alphanumeric(),
            };
            !cut && is_ipv6(&self.chars[i..end])
        })
    }

    /// One letter with any combining marks that follow it.
    pub fn letter_at(&self, i: usize) -> Option<usize> {
        if !self.char(i)?.is_alphabetic() {
            return None;
        }
        let mut j = i + 1;
        while self.char(j).is_some_and(is_mark) {
            j += 1;
        }
        Some(j)
    }

    /// A line break: `\n`, `\r\n` or `\r`.
    pub fn newline_at(&self, i: usize) -> Option<usize> {
        match self.char(i)? {
            '\n' => Some(i + 1),
            '\r' if self.char(i + 1) == Some('\n') => Some(i + 2),
            '\r' => Some(i + 1),
            _ => None,
        }
    }

    /// Start of the text, or right after a line break.
    pub fn is_line_start(&self, i: usize) -> bool {
        match i.checked_sub(1).and_then(|p| self.char(p)) {
            None => true,
            Some('\n') => true,
            // Between `\r` and `\n` is inside one line break, not a line start.
            Some('\r') => self.char(i) != Some('\n'),
            Some(_) => false,
        }
    }

    /// End of the text, or right before a line break.
    pub fn is_line_end(&self, i: usize) -> bool {
        self.char(i).is_none_or(|c| c == '\n' || c == '\r')
    }

    /// End of `lit` if the text at `i` starts with it.
    pub fn literal_at(&self, i: usize, lit: &[char], ci: bool) -> Option<usize> {
        let end = i + lit.len();
        let here = self.chars.get(i..end)?;
        let same = if ci {
            here.iter().zip(lit).all(|(a, b)| a == b || a.to_lowercase().eq(b.to_lowercase()))
        } else {
            here == lit
        };
        same.then_some(end)
    }

    /// Human-readable description of what is at position `i`, for "found …" messages.
    pub fn describe_at(&self, i: usize) -> String {
        let Some(c) = self.char(i) else { return "the end of the text".into() };
        let end = self.word_at(i).or_else(|| self.number_at(i)).unwrap_or(i + 1);
        match c {
            '\n' | '\r' => "a line break".into(),
            // Show what follows the whitespace: "found \" is blue\"" says more than "a space".
            c if c.is_whitespace() => {
                let mut j = i;
                while j < self.len() && j - i < 12 && !self.is_line_end(j) {
                    j += 1;
                }
                format!("{:?}", self.slice(i, j))
            }
            _ => format!("\"{}\"", self.slice(i, end)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn positions_are_characters() {
        let t = Input::new("héllo\n");
        assert_eq!(t.len(), 6);
        assert_eq!(t.slice(0, 5), "héllo");
        assert_eq!(t.byte_span(1, 2), (1, 3));
        assert_eq!(t.byte_pos(6), 7);
    }

    #[test]
    fn runs() {
        let t = Input::new("abc123 3.14 1.2.3 x");
        assert_eq!(t.word_at(0), Some(3));
        assert_eq!(t.word_at(1), None, "never starts inside a run");
        assert_eq!(t.number_at(3), Some(6));
        assert_eq!(t.number_at(4), None);
        assert_eq!(t.number_at(7), Some(11));
        assert_eq!(t.number_at(12), Some(15), "1.2");
        assert_eq!(t.number_at(16), Some(17), "then .3 is punctuation and a new number");
        assert_eq!(t.word_at(18), Some(19));
    }

    #[test]
    fn number_helpers() {
        let t = Input::new("3.14 12kg 0x1F cafeteria 1010 102 199.72.81.55 1.2.3.4.5 256.1.1.1");
        assert_eq!(t.int_at(0), Some(1));
        assert_eq!(t.int_at(2), Some(4), "the fraction is its own INT");
        assert_eq!(t.int_at(3), None, "never starts inside a run");
        assert_eq!(t.int_at(5), Some(7), "letters may follow an INT");
        assert_eq!(t.hex_at(12), Some(14), "after a 0x prefix");
        assert_eq!(t.hex_at(15), None, "cafe is followed by more letters");
        assert_eq!(t.bin_at(25), Some(29));
        assert_eq!(t.bin_at(30), None, "102 is not binary");
        assert_eq!(t.ipv4_at(34), Some(46));
        assert_eq!(t.ipv4_at(47), None, "five parts");
        assert_eq!(t.ipv4_at(57), None, "256 is out of range");
    }

    #[test]
    fn rows_and_columns() {
        let t = Input::new("ab\ncd\r\nef\rg");
        assert_eq!(t.row_col(0), (1, 1));
        assert_eq!(t.row_col(2), (1, 3), "at the line break");
        assert_eq!(t.row_col(3), (2, 1));
        assert_eq!(t.row_col(7), (3, 1), "after \\r\\n");
        assert_eq!(t.row_col(10), (4, 1), "after a lone \\r");
        assert_eq!(t.row_col(11), (4, 2), "at the end");
    }

    #[test]
    fn ipv6() {
        let whole = |s: &str| Input::new(s).ipv6_at(0) == Some(s.chars().count());
        for ok in ["::", "::1", "fe80::1", "2001:DB8:0:0:8:800:200C:417A", "1::", "::ffff:192.0.2.1", "1:2:3:4:5:6:7::"]
        {
            assert!(whole(ok), "{ok}");
        }
        for bad in
            ["1:2:3", ":::", "1::2::3", "12345::", "1:2:3:4:5:6:7:8:9", "::1.2.3", "::256.1.1.1", "1:2:3:4:5:6:7:8::"]
        {
            assert!(!whole(bad), "{bad}");
        }
        let t = Input::new("[::1]:80 2001:db8::1: up 10:30:00 std::x fe80::1g ::1.2.3.4.5");
        assert_eq!(t.ipv6_at(1), Some(4), "inside brackets");
        assert_eq!(t.ipv6_at(9), Some(20), "a trailing `:` is not part of it");
        assert_eq!(t.ipv6_at(25), None, "a time has no `::`");
        assert_eq!(t.ipv6_at(37), None, "preceded by a letter");
        assert_eq!(t.ipv6_at(41), None, "followed by a letter");
        assert_eq!(t.ipv6_at(50), None, "followed by more of a dotted number");
    }

    #[test]
    fn apostrophes_split_words() {
        let t = Input::new("don't");
        assert_eq!(t.word_at(0), Some(3));
        assert_eq!(t.word_at(4), Some(5));
    }

    #[test]
    fn letters_and_marks() {
        let t = Input::new("e\u{301}t東");
        assert_eq!(t.word_at(0), Some(4));
        assert_eq!(t.letter_at(0), Some(2), "letter with its combining accent");
        assert_eq!(t.letter_at(2), Some(3));
        assert_eq!(t.letter_at(3), Some(4));
        assert_eq!(t.letter_at(1), None, "a lone mark is not a letter");
    }

    #[test]
    fn line_breaks() {
        let t = Input::new("a\r\nb\rc\nd");
        assert_eq!(t.newline_at(1), Some(3));
        assert_eq!(t.newline_at(2), Some(3));
        assert_eq!(t.newline_at(4), Some(5));
        assert_eq!(t.newline_at(6), Some(7));
        assert!(t.is_line_start(0) && t.is_line_start(3) && t.is_line_start(5) && t.is_line_start(7));
        assert!(!t.is_line_start(2), "between \\r and \\n");
        assert!(t.is_line_end(1) && t.is_line_end(4) && t.is_line_end(8));
        assert!(!t.is_line_end(0));
    }

    #[test]
    fn literals() {
        let t = Input::new("Roses are red");
        let lit: Vec<char> = " are ".chars().collect();
        assert_eq!(t.literal_at(5, &lit, false), Some(10));
        assert_eq!(t.literal_at(4, &lit, false), None);
        let ci: Vec<char> = "roses".chars().collect();
        assert_eq!(t.literal_at(0, &ci, false), None);
        assert_eq!(t.literal_at(0, &ci, true), Some(5));
        assert_eq!(t.literal_at(10, &"red!".chars().collect::<Vec<_>>(), false), None, "past the end");
    }

    #[test]
    fn descriptions() {
        let t = Input::new("ab 12\n\tx");
        assert_eq!(t.describe_at(0), "\"ab\"");
        assert_eq!(t.describe_at(2), "\" 12\"");
        assert_eq!(t.describe_at(3), "\"12\"");
        assert_eq!(t.describe_at(5), "a line break");
        assert_eq!(t.describe_at(6), "\"\\tx\"");
        assert_eq!(t.describe_at(8), "the end of the text");
    }

    #[test]
    fn empty() {
        let t = Input::new("");
        assert!(t.is_empty());
        assert!(t.is_line_start(0) && t.is_line_end(0));
        assert_eq!(t.describe_at(0), "the end of the text");
    }
}
