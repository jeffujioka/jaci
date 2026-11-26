//! The banner's layout: the art on the left and, after the padding, the text
//! panel, whose lines wrap at the panel's width.

use std::{iter, mem};

use unicode_width::UnicodeWidthChar;

use crate::ansi::{self, ESC, RESET, Token};
use crate::config::{Align, Valign};

/// The narrowest text panel the banner shows; a narrower one leaves the art
/// alone.
const MIN_PANEL_COLUMNS: usize = 20;

/// The banner's rows, without newlines: each art row as stored, followed,
/// after a reset when the row does not end with one, by `padding` spaces and
/// its text row, and the text rows left below the art indented by the art's
/// width and `padding`.
///
/// The panel is `terminal_width` minus the art's width and `padding` columns
/// wide; under 20 columns the rows are the art's alone. Each line
/// of `text` wraps at the panel's width, in display width with escape
/// sequences counting as nothing, each piece in the style and colours its
/// line has there and ending with a reset when it holds an escape, and
/// every piece is placed by `align`. `valign` places text with fewer rows
/// than the art. No row ends in spaces.
pub fn banner(
    art: &[String],
    text: &[String],
    padding: u32,
    align: Align,
    valign: Valign,
    terminal_width: usize,
) -> Vec<String> {
    let art_columns = art.first().map_or(0, |row| ansi::visible_width(row));
    // A padding that leaves no room, even one past usize, leaves no panel.
    let room = usize::try_from(padding).ok().and_then(|padding| {
        let panel = terminal_width
            .checked_sub(art_columns)?
            .checked_sub(padding)?;
        Some((padding, panel))
    });
    let Some((padding, panel)) = room.filter(|&(_, panel)| panel >= MIN_PANEL_COLUMNS) else {
        return art.to_vec();
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
            let art_row = art.get(row);
            let piece = row
                .checked_sub(first)
                .and_then(|index| pieces.get(index))
                .filter(|piece| !piece.text.is_empty());
            let Some(piece) = piece else {
                return art_row.cloned().unwrap_or_default();
            };
            let offset = match align {
                Align::Left => 0,
                Align::Center => (panel - piece.width) / 2,
                Align::Right => panel - piece.width,
            };
            let art_len = art_row.map_or(art_columns, |row| row.len() + RESET.len());
            let mut line = String::with_capacity(art_len + padding + offset + piece.text.len());
            match art_row {
                Some(art_row) => {
                    line.push_str(art_row);
                    // Keeps the art's last colours out of the padding.
                    if !art_row.ends_with(RESET) {
                        line.push_str(RESET);
                    }
                }
                None => line.extend(iter::repeat_n(' ', art_columns)),
            }
            line.extend(iter::repeat_n(' ', padding + offset));
            line.push_str(&piece.text);
            line
        })
        .collect()
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
/// piece after the first starts with the line's leading escapes, then every
/// SGR sequence of the line before it, in order, so it shows the style and
/// colours the line has where it starts. A piece with no characters is
/// empty.
fn wrap(line: &str, panel: usize) -> Vec<Piece> {
    let style_len = ansi::tokens(line)
        .map_while(|token| match token {
            Token::Escape(escape) => Some(escape.len()),
            Token::Char(_) => None,
        })
        .sum();
    let (style, body) = line.split_at(style_len);
    // Each piece ends with a reset of its own.
    let body = body.strip_suffix(RESET).unwrap_or(body);
    // The escapes that start the next piece.
    let mut active = style.to_owned();
    let mut pieces = Vec::new();
    let mut text = active.clone();
    let mut width = 0;
    let mut characters = false;
    for token in ansi::tokens(body) {
        match token {
            Token::Escape(escape) => {
                text.push_str(escape);
                if ansi::is_sgr(escape) {
                    active.push_str(escape);
                }
            }
            Token::Char(c) => {
                let char_width = c.width().unwrap_or(0);
                if width + char_width > panel {
                    let full = mem::replace(&mut text, active.clone());
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
