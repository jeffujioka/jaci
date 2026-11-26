//! The escape sequences of jaci's output: the reset, the 24-bit colours, and
//! text read back as escapes and the characters between them.

use std::fmt::Write as _;
use std::iter;

use unicode_width::UnicodeWidthChar;

/// Ends every style and colour: `ESC[0m`.
pub(crate) const RESET: &str = "\x1b[0m";
/// The start of a CSI sequence: ESC and `[`.
const CSI: &str = "\x1b[";
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
    /// A CSI sequence: `ESC [` up to its final byte in `@`..=`~`.
    Escape(&'a str),
    /// A character outside escape sequences.
    Char(char),
}

/// The tokens of `text`, in order.
pub(crate) fn tokens(text: &str) -> impl Iterator<Item = Token<'_>> {
    let mut rest = text;
    iter::from_fn(move || {
        if let Some(sequence) = rest.strip_prefix(CSI) {
            // The final byte is ASCII, one byte long.
            let end = sequence
                .find(|c| ('@'..='~').contains(&c))
                .map_or(sequence.len(), |index| index + 1);
            let (escape, tail) = rest.split_at(CSI.len() + end);
            rest = tail;
            return Some(Token::Escape(escape));
        }
        let first = rest.chars().next()?;
        rest = &rest[first.len_utf8()..];
        Some(Token::Char(first))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_csi_sequences_are_escapes() {
        assert_eq!(visible_width("\x1b[38;2;1;2;3mab\x1b[0m"), 2);
        // ESC alone is a control character, of no width; what follows it
        // outside a CSI sequence is text.
        assert_eq!(visible_width("\x1b7ab"), 3);
    }

    #[test]
    fn colours_are_24_bit_escapes() {
        let mut text = String::new();

        push_foreground(&mut text, [1, 2, 3]);
        push_background(&mut text, [4, 5, 6]);

        assert_eq!(text, "\x1b[38;2;1;2;3m\x1b[48;2;4;5;6m");
    }
}
