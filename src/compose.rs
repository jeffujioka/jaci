//! The banner's layout: the art and the text panel side by side, the panel
//! below the art, the art alone or the panel alone, whichever first fits
//! the pane, and the text panel's lines, which wrap at the panel's width.

use std::{iter, mem};

use unicode_width::UnicodeWidthChar;

use crate::ansi::{self, ActiveSgr, ESC, RESET, Token};
use crate::config::{Align, Encode, Valign};
use crate::size;

/// The narrowest text panel the banner shows; a narrower one leaves the art
/// alone.
const MIN_PANEL_COLUMNS: usize = 20;
/// The rows the banner leaves below it for the prompt.
const PROMPT_ROWS: usize = 2;

/// The text panel beside the art: its width and the padding before it.
/// Only [`panel_width`] and [`layout`] make one, so it fits the terminal
/// beside the art and the padding.
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

/// The pane the banner is shown in: its columns and rows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Pane {
    pub columns: usize,
    pub rows: usize,
}

/// How the banner shows the art and the text panel in a pane, and the art's
/// width in columns.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Layout {
    /// The art with the panel beside it, after the panel's padding.
    SideBySide { art_columns: usize, panel: Panel },
    /// The art, an empty row, then the panel `panel_width` columns wide,
    /// the narrower of the two centred on the wider.
    PanelBelow {
        art_columns: usize,
        panel_width: usize,
    },
    /// The art without the panel.
    ArtAlone { art_columns: usize },
    /// The panel `panel_width` columns wide, the pane's width, without the
    /// art.
    PanelAlone { panel_width: usize },
}

/// The first layout that fits `pane`, for a saved image `image` pixels
/// wide and high, art from `encode.min_width` to `encode.max_width` columns
/// wide whose rows follow [`size::art_rows`] with `encode.ratio`, `padding`
/// columns before a panel beside the art, and the panel lines `text`. Two
/// rows are left below the banner for the prompt, and the art takes the
/// largest width its layout fits:
///
/// - side by side when the columns left after the art and the padding hold
///   the widest line of `text`, in display width, and the art fits the rows;
/// - else the panel below, when the panel lines are not all empty and art
///   no wider than the pane, an empty row and the lines wrapped at the
///   lesser of the pane's columns and the widest line fit the rows;
/// - else the art alone, when art no wider than the pane fits the rows;
/// - else the panel alone, as wide as the pane.
pub fn layout(
    pane: Pane,
    encode: &Encode,
    padding: u32,
    image: (u32, u32),
    text: &[String],
) -> Layout {
    // Whether art `art_columns` wide fits the pane's rows with
    // `other_rows` more and the prompt's.
    let fits = |art_columns: u32, other_rows: usize| {
        let art_rows = size::art_rows(art_columns, image, encode.ratio);
        usize::try_from(art_rows).is_ok_and(|art_rows| {
            art_rows
                .checked_add(other_rows + PROMPT_ROWS)
                .is_some_and(|rows| rows <= pane.rows)
        })
    };
    // The art widths the size limits allow up to `most` columns, the
    // largest first.
    let widths = |most: usize| {
        let most = u32::try_from(most).unwrap_or(u32::MAX);
        (encode.min_width..=encode.max_width.min(most)).rev()
    };
    let widest = text
        .iter()
        .map(|line| ansi::visible_width(line))
        .max()
        .unwrap_or(0);
    let side_by_side = usize::try_from(padding).ok().and_then(|padding| {
        // Art at most this wide leaves the widest line room after the
        // padding.
        let beside = pane.columns.checked_sub(padding)?.checked_sub(widest)?;
        let art_columns = widths(beside).find(|&columns| fits(columns, 0))? as usize;
        Some(Layout::SideBySide {
            art_columns,
            panel: Panel {
                width: pane.columns - art_columns - padding,
                padding,
            },
        })
    });
    if let Some(layout) = side_by_side {
        return layout;
    }
    let panel_width = widest.min(pane.columns);
    let panel_rows: usize = text.iter().map(|line| wrap(line, panel_width).len()).sum();
    if widest > 0
        && let Some(art_columns) =
            widths(pane.columns).find(|&columns| fits(columns, 1 + panel_rows))
    {
        return Layout::PanelBelow {
            art_columns: art_columns as usize,
            panel_width,
        };
    }
    if let Some(art_columns) = widths(pane.columns).find(|&columns| fits(columns, 0)) {
        return Layout::ArtAlone {
            art_columns: art_columns as usize,
        };
    }
    Layout::PanelAlone {
        panel_width: pane.columns,
    }
}

/// The banner's rows in `layout`, without newlines, from `art`, the rows of
/// the art at the layout's width, and the panel lines `text`. Side by side
/// they are as [`banner`] gives them. Each line of `text` wraps at the
/// panel's width and every piece is placed by `align` across it; `valign`
/// places text with fewer rows than the art beside it. No row ends in
/// spaces.
pub fn rows(
    layout: Layout,
    art: &[String],
    text: &[String],
    align: Align,
    valign: Valign,
) -> Vec<String> {
    match layout {
        Layout::SideBySide { art_columns, panel } => {
            banner(art, art_columns, text, Some(panel), align, valign)
        }
        Layout::PanelBelow {
            art_columns,
            panel_width,
        } => {
            let width = art_columns.max(panel_width);
            let indent = (width - art_columns) / 2;
            let panel = Panel {
                width: panel_width,
                padding: (width - panel_width) / 2,
            };
            let mut rows: Vec<String> = art.iter().map(|row| art_row(indent, row, 0)).collect();
            rows.push(String::new());
            rows.extend(banner(&[], 0, text, Some(panel), align, Valign::Top));
            rows
        }
        Layout::ArtAlone { art_columns } => banner(art, art_columns, &[], None, align, valign),
        Layout::PanelAlone { panel_width } => {
            let panel = Panel {
                width: panel_width,
                padding: 0,
            };
            banner(&[], 0, text, Some(panel), align, valign)
        }
    }
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
        return art.iter().map(|row| art_row(0, row, 0)).collect();
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
                return stored.map_or_else(String::new, |stored| art_row(0, stored, 0));
            };
            let offset = match align {
                Align::Left => 0,
                // A wide character moved whole can make a piece wider than
                // a 1-column panel.
                Align::Center => panel.saturating_sub(piece.width) / 2,
                Align::Right => panel.saturating_sub(piece.width),
            };
            let tail = padding + offset + piece.text.len();
            let mut line = match stored {
                Some(stored) => art_row(0, stored, tail),
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

/// `row` of the art as the banner shows it, after `indent` spaces and with
/// room for `extra` more bytes: as stored, then a reset when it holds an
/// escape and does not end with one.
fn art_row(indent: usize, row: &str, extra: usize) -> String {
    let mut line = String::with_capacity(indent + row.len() + RESET.len() + extra);
    line.extend(iter::repeat_n(' ', indent));
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
