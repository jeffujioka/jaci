//! The banner's layout: the art on the left and, after the padding, the text
//! panel, whose lines wrap at the panel's width.

use std::iter;

use unicode_width::UnicodeWidthChar;

use crate::art_file::{self, Token};
use crate::config::{Align, Valign};

const RESET: &str = "\x1b[0m";

/// The narrowest text panel the banner shows; a narrower one leaves the art
/// alone.
const MIN_PANEL_COLUMNS: usize = 20;

/// The banner's rows, without newlines: each art row as stored, followed by
/// `padding` spaces and its text row, and the text rows left below the art
/// indented by the art's width and `padding`.
///
/// The panel is `terminal_width` minus the art's width and `padding` columns
/// wide; under 20 columns the rows are the art's alone. Each line
/// of `text` wraps at the panel's width, in display width with escape
/// sequences counting as nothing, and every piece is placed by `align`.
/// `valign` places text with fewer rows than the art. No row ends in spaces.
pub fn banner(
    art: &[String],
    text: &[String],
    padding: usize,
    align: Align,
    valign: Valign,
    terminal_width: usize,
) -> Vec<String> {
    let art_columns = art.first().map_or(0, |row| art_file::visible_width(row));
    let panel = terminal_width.saturating_sub(art_columns + padding);
    if panel < MIN_PANEL_COLUMNS {
        return art.to_vec();
    }
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
            let mut line = String::with_capacity(
                art_row.map_or(art_columns, String::len) + padding + offset + piece.text.len(),
            );
            match art_row {
                Some(art_row) => line.push_str(art_row),
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
    /// The text, with the line's style around it; empty when the row has no
    /// text.
    text: String,
    /// The display width of the text.
    width: usize,
}

/// Wraps `line` into pieces at most `panel` columns wide, moving a wide
/// character that does not fit to the next piece whole. A styled line, one
/// starting with escape sequences, has them at the start of every piece and a
/// reset at its end.
fn wrap(line: &str, panel: usize) -> Vec<Piece> {
    let style_len = art_file::tokens(line)
        .map_while(|token| match token {
            Token::Escape(escape) => Some(escape.len()),
            Token::Char(_) => None,
        })
        .sum();
    let (style, body) = line.split_at(style_len);
    let body = if style.is_empty() {
        body
    } else {
        body.strip_suffix(RESET).unwrap_or(body)
    };
    let piece = |text: &str, width| Piece {
        text: if style.is_empty() || text.is_empty() {
            text.to_owned()
        } else {
            format!("{style}{text}{RESET}")
        },
        width,
    };
    let mut pieces = Vec::new();
    let mut text = String::new();
    let mut width = 0;
    for token in art_file::tokens(body) {
        match token {
            Token::Escape(escape) => text.push_str(escape),
            Token::Char(c) => {
                let char_width = c.width().unwrap_or(0);
                if width + char_width > panel {
                    pieces.push(piece(&text, width));
                    text.clear();
                    width = 0;
                }
                text.push(c);
                width += char_width;
            }
        }
    }
    pieces.push(piece(&text, width));
    pieces
}
