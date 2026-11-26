//! The banner's layout: the art on the left and, after the padding, the text
//! panel, whose lines wrap at the panel's width.

use std::{iter, mem};

use unicode_width::UnicodeWidthChar;

use crate::ansi::{self, ActiveSgr, ESC, RESET, Token};
use crate::config::{Align, Valign};

/// The narrowest text panel the banner shows; a narrower one leaves the art
/// alone.
const MIN_PANEL_COLUMNS: usize = 20;

/// The text panel beside the art: its width and the padding before it.
/// Only [`panel_width`] makes one, so its width is at least 20 columns and
/// fits the terminal beside the art and the padding.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Panel {
    width: usize,
    padding: usize,
}

impl Panel {
    /// The panel's width in columns.
    pub fn width(self) -> usize {
        self.width
    }
}

/// The text panel beside art `art_columns` wide, after `padding` columns, on
/// a terminal `terminal_width` columns wide: as wide as what is left of the
/// terminal, or `None` when that is under 20 columns and the banner shows
/// the art alone.
pub fn panel_width(art_columns: usize, padding: u32, terminal_width: usize) -> Option<Panel> {
    // A padding that leaves no room, even one past usize, leaves no panel.
    let padding = usize::try_from(padding).ok()?;
    let width = terminal_width
        .checked_sub(art_columns)?
        .checked_sub(padding)?;
    (width >= MIN_PANEL_COLUMNS).then_some(Panel { width, padding })
}

/// The banner's rows, without newlines: each art row, followed by the
/// panel's padding in spaces and its text row, and the text rows left below
/// the art, indented by `art_columns`, the art's width, and the padding. An
/// art row is shown as stored, with a reset after it when it holds an
/// escape and does not end with one, so its colours stop with it.
///
/// `panel` is the one [`panel_width`] gives for `art_columns`; without one
/// the rows are the art's alone. Each line of `text` wraps at the panel's
/// width, in display width with escape sequences counting as nothing, each
/// piece in the style and colours its line has there and ending with a
/// reset when it holds an escape, and every piece is placed by `align`.
/// `valign` places text with fewer rows than the art. No row ends in
/// spaces.
pub fn banner(
    art: &[String],
    art_columns: usize,
    text: &[String],
    panel: Option<Panel>,
    align: Align,
    valign: Valign,
) -> Vec<String> {
    let Some(Panel {
        width: panel,
        padding,
    }) = panel
    else {
        return art.iter().map(|row| art_row(row, 0)).collect();
    };
    let pieces: Vec<Piece> = text.iter().flat_map(|line| wrap(line, panel)).collect();
    let spare = art.len().saturating_sub(pieces.len());
    let first = match valign {
        Valign::Top => 0,
        Valign::Center => spare / 2,
        Valign::Bottom => spare,
    };
    let height = art.len().max(first + pieces.len());
    (0..height)
        .map(|row| {
            let stored = art.get(row);
            let piece = row
                .checked_sub(first)
                .and_then(|index| pieces.get(index))
                .filter(|piece| !piece.text.is_empty());
            let Some(piece) = piece else {
                return stored.map_or_else(String::new, |stored| art_row(stored, 0));
            };
            let offset = match align {
                Align::Left => 0,
                Align::Center => (panel - piece.width) / 2,
                Align::Right => panel - piece.width,
            };
            let tail = padding + offset + piece.text.len();
            let mut line = match stored {
                Some(stored) => art_row(stored, tail),
                None => {
                    let mut line = String::with_capacity(art_columns + tail);
                    line.extend(iter::repeat_n(' ', art_columns));
                    line
                }
            };
            line.extend(iter::repeat_n(' ', padding + offset));
            line.push_str(&piece.text);
            line
        })
        .collect()
}

/// `row` of the art as the banner shows it, with room for `extra` more
/// bytes: as stored, then a reset when it holds an escape and does not end
/// with one.
fn art_row(row: &str, extra: usize) -> String {
    let mut line = String::with_capacity(row.len() + RESET.len() + extra);
    line.push_str(row);
    if row.contains(ESC) && !row.ends_with(RESET) {
        line.push_str(RESET);
    }
    line
}

/// One row of a wrapped text line.
struct Piece {
    /// The text, in the style and colours of its line; empty when the row
    /// has no characters.
    text: String,
    /// The display width of the text.
    width: usize,
}

impl Piece {
    /// The piece of `text`, `width` columns wide: empty without
    /// `characters`, and ending with a reset when `text` holds an escape.
    fn new(mut text: String, width: usize, characters: bool) -> Self {
        if !characters {
            text.clear();
        } else if text.contains(ESC) {
            text.push_str(RESET);
        }
        Self { text, width }
    }
}

/// Wraps `line` into pieces at most `panel` columns wide, moving a wide
/// character that does not fit to the next piece whole. A piece holding an
/// escape sequence ends with a reset, so its colours stop with it. Every
/// piece after the first starts with the SGR sequences of the line before
/// it since their last reset, in order, so it shows the style and colours
/// the line has where it starts. A piece with no characters is empty.
fn wrap(line: &str, panel: usize) -> Vec<Piece> {
    // Each piece ends with a reset of its own.
    let line = line.strip_suffix(RESET).unwrap_or(line);
    // The escapes that start the next piece.
    let mut active = ActiveSgr::default();
    let mut pieces = Vec::new();
    let mut text = String::new();
    let mut width = 0;
    let mut characters = false;
    for token in ansi::tokens(line) {
        match token {
            Token::Escape(escape) => {
                text.push_str(escape);
                active.record(escape);
            }
            Token::Char(c) => {
                let char_width = c.width().unwrap_or(0);
                if width + char_width > panel {
                    let full = mem::replace(&mut text, active.as_str().to_owned());
                    pieces.push(Piece::new(full, width, characters));
                    width = 0;
                }
                text.push(c);
                width += char_width;
                characters = true;
            }
        }
    }
    pieces.push(Piece::new(text, width, characters));
    pieces
}
