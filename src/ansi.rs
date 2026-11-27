//! The escape sequences of jaci's output: the reset, the 24-bit colours, and
//! text read back as escapes and the characters between them.

use std::fmt::Write as _;
use std::iter;

use unicode_width::UnicodeWidthChar;

/// Ends every style and colour: `ESC[0m`.
pub(crate) const RESET: &str = "\x1b[0m";
/// Draws text bold: `ESC[1m`.
pub(crate) const BOLD: &str = "\x1b[1m";
/// Draws text dim: `ESC[2m`.
pub(crate) const DIM: &str = "\x1b[2m";
/// The escape character that starts every escape sequence.
pub(crate) const ESC: &str = "\x1b";
const ESC_BYTE: u8 = ESC.as_bytes()[0];
/// The start of a CSI sequence: ESC and `[`.
const CSI: &str = "\x1b[";
/// The second bytes of the control strings: OSC, DCS, SOS, PM and APC.
const STRING_STARTS: [u8; 5] = [b']', b'P', b'X', b'^', b'_'];
/// The string terminator, which ends a control string: ESC and `\`.
const ST: &str = "\x1b\\";
/// The bell, which also ends a control string.
const BEL: u8 = 0x07;
const FOREGROUND: u8 = 38;
const BACKGROUND: u8 = 48;

/// Appends `ESC[38;2;R;G;Bm`, which draws text in `colour`.
pub(crate) fn push_foreground(text: &mut String, colour: [u8; 3]) {
    push_colour(text, FOREGROUND, colour);
}

/// Appends `ESC[48;2;R;G;Bm`, which fills the cell behind the text with
/// `colour`.
pub(crate) fn push_background(text: &mut String, colour: [u8; 3]) {
    push_colour(text, BACKGROUND, colour);
}

/// Appends the escape that sets `colour` on `layer`, 38 for the foreground
/// or 48 for the background.
fn push_colour(text: &mut String, layer: u8, [r, g, b]: [u8; 3]) {
    write!(text, "{CSI}{layer};2;{r};{g};{b}m").expect("writing to a String cannot fail");
}

/// The display width of `text`, skipping its escape sequences.
pub(crate) fn visible_width(text: &str) -> usize {
    tokens(text)
        .map(|token| match token {
            Token::Escape(_) => 0,
            Token::Char(c) => c.width().unwrap_or(0),
        })
        .sum()
}

/// One piece of text as a terminal reads it.
pub(crate) enum Token<'a> {
    /// An escape sequence: a CSI sequence, `ESC [` up to its final byte in
    /// `@`..=`~`; a control string, OSC `ESC ]`, DCS `ESC P`, SOS `ESC X`,
    /// PM `ESC ^` or APC `ESC _`, up to a BEL or the string terminator
    /// `ESC \`; or any other escape, `ESC`, any intermediate bytes in
    /// ` `..=`/`, then a final byte in `0`..=`~`, such as `ESC ( B` or
    /// `ESC 7`.
    Escape(&'a str),
    /// A character outside escape sequences.
    Char(char),
}

/// The tokens of `text`, in order.
pub(crate) fn tokens(text: &str) -> impl Iterator<Item = Token<'_>> {
    let mut rest = text;
    iter::from_fn(move || {
        if let Some(length) = csi_length(rest)
            .or_else(|| string_length(rest))
            .or_else(|| escape_length(rest))
        {
            let (escape, tail) = rest.split_at(length);
            rest = tail;
            return Some(Token::Escape(escape));
        }
        let first = rest.chars().next()?;
        rest = &rest[first.len_utf8()..];
        Some(Token::Char(first))
    })
}

/// The length in bytes of the CSI sequence that starts `text`, up to the end
/// of `text` when it has no final byte.
fn csi_length(text: &str) -> Option<usize> {
    let sequence = text.strip_prefix(CSI)?;
    // The final byte is ASCII, one byte long.
    let end = sequence
        .find(|c| ('@'..='~').contains(&c))
        .map_or(sequence.len(), |index| index + 1);
    Some(CSI.len() + end)
}

/// The length in bytes of the control string that starts `text`: up to and
/// with its BEL or string terminator, before an ESC that starts anything
/// else, which ends it too, or up to the end of `text`.
fn string_length(text: &str) -> Option<usize> {
    let start = text.strip_prefix(ESC)?.bytes().next()?;
    if !STRING_STARTS.contains(&start) {
        return None;
    }
    // The ESC and its second byte are ASCII, two bytes long.
    let sequence = &text[2..];
    // BEL and ESC are ASCII, so every end lies on a character boundary.
    let end = match sequence
        .bytes()
        .position(|byte| byte == BEL || byte == ESC_BYTE)
    {
        None => sequence.len(),
        Some(index) if sequence.as_bytes()[index] == BEL => index + 1,
        Some(index) if sequence[index..].starts_with(ST) => index + ST.len(),
        Some(index) => index,
    };
    Some(2 + end)
}

/// Whether `escape` is an SGR sequence, `ESC [`, parameters of digits, `;`
/// and `:`, then `m`, which sets the style and colours of the text after it.
pub(crate) fn is_sgr(escape: &str) -> bool {
    sgr_parameters(escape).is_some()
}

/// The parameters of `escape` when it is an SGR sequence, such as `1;31` of
/// `ESC[1;31m`.
fn sgr_parameters(escape: &str) -> Option<&str> {
    escape
        .strip_prefix(CSI)
        .and_then(|sequence| sequence.strip_suffix('m'))
        .filter(|parameters| {
            parameters
                .bytes()
                .all(|byte| byte.is_ascii_digit() || byte == b';' || byte == b':')
        })
}

/// The 24-bit colours `escape` sets when it is an SGR sequence: one for each
/// `38;2;R;G;B` or `48;2;R;G;B` group among its parameters, in order.
pub(crate) fn colours(escape: &str) -> impl Iterator<Item = [u8; 3]> + '_ {
    let mut parameters = sgr_parameters(escape)
        .into_iter()
        .flat_map(|parameters| parameters.split(';'));
    iter::from_fn(move || {
        loop {
            if !matches!(parameters.next()?.parse(), Ok(FOREGROUND | BACKGROUND)) {
                continue;
            }
            if parameters.next()?.parse() != Ok(2u8) {
                continue;
            }
            let mut channel = || parameters.next().and_then(|channel| channel.parse().ok());
            if let (Some(r), Some(g), Some(b)) = (channel(), channel(), channel()) {
                return Some([r, g, b]);
            }
        }
    })
}

/// The SGR sequences in effect after the text recorded so far: every one
/// since the last reset, in order, so replaying them gives the style and
/// colours the terminal has there.
#[derive(Debug, Default)]
pub(crate) struct ActiveSgr(String);

impl ActiveSgr {
    /// Records `escape` when it is an SGR sequence. One whose first
    /// parameter is a reset, `ESC[m`, `ESC[0m` or `ESC[0;1m`, ends every
    /// sequence before it, and leaves nothing to replay when all its
    /// parameters are resets.
    pub(crate) fn record(&mut self, escape: &str) {
        let Some(parameters) = sgr_parameters(escape) else {
            return;
        };
        // An empty parameter, or one of zeros, is 0: the reset.
        let is_reset = |parameter: &str| parameter.bytes().all(|byte| byte == b'0');
        let mut parameters = parameters.split(';');
        if parameters.next().is_some_and(is_reset) {
            self.0.clear();
            if parameters.all(is_reset) {
                return;
            }
        }
        self.0.push_str(escape);
    }

    /// The recorded sequences, in order.
    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }
}

/// The length in bytes of the escape that starts `text`, an nF escape with
/// its intermediate bytes or a two-character escape, if one does.
fn escape_length(text: &str) -> Option<usize> {
    let sequence = text.strip_prefix(ESC)?.as_bytes();
    let intermediates = sequence
        .iter()
        .take_while(|byte| (b' '..=b'/').contains(byte))
        .count();
    let final_byte = *sequence.get(intermediates)?;
    // Every byte is ASCII, so the length ends on a character boundary.
    (b'0'..=b'~')
        .contains(&final_byte)
        .then_some(ESC.len() + intermediates + 1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn csi_sequences_are_escapes() {
        assert_eq!(visible_width("\x1b[38;2;1;2;3mab\x1b[0m"), 2);
        // ESC alone is a control character, of no width; what follows it
        // outside an escape sequence is text.
        assert_eq!(visible_width("\x1b\u{e9}ab"), 3);
    }

    #[test]
    fn two_character_escapes_have_no_width() {
        assert_eq!(visible_width("\x1b7a\x1bcb\x1b=c\x1b8"), 3);
    }

    #[test]
    fn control_strings_end_at_bel_or_string_terminator() {
        assert_eq!(visible_width("\x1b]0;title\x07ab"), 2);
        assert_eq!(visible_width("\x1b]0;title\x1b\\ab"), 2);
        assert_eq!(
            visible_width("\x1bPq#0\x1b\\a\x1bXs\x07b\x1b^p\x07c\x1b_a\x07d"),
            4
        );
        // An ESC that starts anything else ends it before itself.
        assert_eq!(visible_width("\x1b]0;title\x1b[1mab"), 2);
        assert_eq!(visible_width("\x1bPunterminated"), 0);
    }

    #[test]
    fn sgr_sequences_are_csi_ending_in_m() {
        assert!(is_sgr("\x1b[m"));
        assert!(is_sgr("\x1b[1;38:2::1:2:3m"));
        assert!(!is_sgr("\x1b[2J"));
        assert!(!is_sgr("\x1b[?25m"));
        assert!(!is_sgr("\x1b(B"));
    }

    #[test]
    fn active_sgr_restarts_after_a_reset() {
        let mut active = ActiveSgr::default();
        // Each step's escapes, then the sequences active after them.
        let steps: [(&[&str], &str); 5] = [
            (
                &["\x1b[1m", "\x1b(B", "\x1b[2J", "\x1b[38:2::1:2:3m"],
                "\x1b[1m\x1b[38:2::1:2:3m",
            ),
            (&["\x1b[m"], ""),
            (&["\x1b[4m", "\x1b[00;0m"], ""),
            (&["\x1b[4m", "\x1b[0;32m", "\x1b[1m"], "\x1b[0;32m\x1b[1m"),
            (&["\x1b[;7m"], "\x1b[;7m"),
        ];

        for (escapes, expected) in steps {
            for escape in escapes {
                active.record(escape);
            }
            assert_eq!(active.as_str(), expected, "after {escapes:?}");
        }
    }

    #[test]
    fn nf_escapes_have_no_width() {
        // `tput sgr0` on xterm: ESC ( B selects the ASCII character set.
        assert_eq!(visible_width("\x1b(Bab"), 2);
        assert_eq!(visible_width("\x1b(B\x1b[mab"), 2);
    }

    #[test]
    fn colours_are_24_bit_escapes() {
        let mut text = String::new();

        push_foreground(&mut text, [1, 2, 3]);
        push_background(&mut text, [4, 5, 6]);

        assert_eq!(text, "\x1b[38;2;1;2;3m\x1b[48;2;4;5;6m");
    }

    #[test]
    fn sgr_colours_are_24_bit_groups() {
        let found: Vec<[u8; 3]> = ["\x1b[1;38;2;1;2;3;48;2;4;5;6m", "\x1b[38;5;7m", "\x1b(B"]
            .into_iter()
            .flat_map(colours)
            .collect();

        assert_eq!(found, [[1, 2, 3], [4, 5, 6]]);
    }
}
