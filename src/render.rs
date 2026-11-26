//! The render modes: each turns a pixel grid, already at the mode's size,
//! into rows of character cells drawn with 24-bit colour escapes.
//!
//! A pixel with alpha 0 is transparent and every other pixel is opaque. A
//! cell that reaches past the image's edge counts the missing pixels as
//! transparent. A cell with no opaque pixel is a space with no escape; every
//! other cell ends in a reset.

use image::RgbaImage;

use crate::ansi::{self, RESET};
use crate::named::Named;

pub(crate) const DEFAULT_CHARSET: &str = " .:-=+*#%@";
const BRAILLE_BLANK: u32 = 0x2800;
/// Each braille dot as its pixel's offset in the 2×4 cell and its bit:
/// dots 1, 2, 3, 7 down the left column, 4, 5, 6, 8 down the right.
const BRAILLE_DOTS: [(u32, u32, u32); 8] = [
    (0, 0, 0x01),
    (0, 1, 0x02),
    (0, 2, 0x04),
    (0, 3, 0x40),
    (1, 0, 0x08),
    (1, 1, 0x10),
    (1, 2, 0x20),
    (1, 3, 0x80),
];

type Rgb = [u8; 3];

/// A render mode, which draws each character cell from a fixed grid of
/// pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// [`halfblock`]: 1×2 pixels per cell.
    Halfblock,
    /// [`ascii`]: 1×1 pixel per cell.
    Ascii,
    /// [`braille`]: 2×4 pixels per cell.
    Braille,
}

impl Named for Mode {
    /// In the order the help and the messages list them.
    const ALL: &'static [Self] = &[Self::Halfblock, Self::Ascii, Self::Braille];

    fn name(self) -> &'static str {
        match self {
            Self::Halfblock => "halfblock",
            Self::Ascii => "ascii",
            Self::Braille => "braille",
        }
    }
}

impl Mode {
    /// The width and height in pixels of one character cell.
    pub const fn cell_size(self) -> (u32, u32) {
        match self {
            Self::Halfblock => (1, 2),
            Self::Ascii => (1, 1),
            Self::Braille => (2, 4),
        }
    }
}

/// The characters ascii mode picks from, darkest first; never empty.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Charset(Vec<char>);

impl Charset {
    /// The charset of `text`'s characters, or `None` when `text` is empty.
    pub fn new(text: &str) -> Option<Self> {
        let chars: Vec<char> = text.chars().collect();
        (!chars.is_empty()).then_some(Self(chars))
    }

    /// The character at index ⌊L × n ÷ 256⌋, with L = 0.299R + 0.587G +
    /// 0.114B and n the charset length, computed exactly in integers as
    /// ⌊(299R + 587G + 114B) × n ÷ 256 000⌋ so no boundary rounds down.
    fn pick(&self, [r, g, b]: Rgb) -> char {
        let luminance_thousandths = 299 * u64::from(r) + 587 * u64::from(g) + 114 * u64::from(b);
        // L is at most 255, below 256, so the index stays below n.
        let index = luminance_thousandths * self.0.len() as u64 / 256_000;
        self.0[usize::try_from(index).expect("the index is below the charset length")]
    }
}

impl Default for Charset {
    /// ` .:-=+*#%@`
    fn default() -> Self {
        Self(DEFAULT_CHARSET.chars().collect())
    }
}

/// Renders `image` with one cell per two stacked pixels: the upper half block
/// in the top pixel's colour over the bottom pixel's as background, or the
/// one half block whose pixel is opaque.
pub fn halfblock(image: &RgbaImage) -> Vec<String> {
    render_cells(image, Mode::Halfblock.cell_size(), |row, x, y| {
        match (opaque(image, x, y), opaque(image, x, y + 1)) {
            (Some(top), Some(bottom)) => {
                ansi::push_foreground(row, top);
                ansi::push_background(row, bottom);
                row.push('▀');
                row.push_str(RESET);
            }
            (Some(top), None) => push_cell(row, top, '▀'),
            (None, Some(bottom)) => push_cell(row, bottom, '▄'),
            (None, None) => row.push(' '),
        }
    })
}

/// Renders `image` with one cell per pixel: the `charset` character its
/// luminance picks, in its colour.
pub fn ascii(image: &RgbaImage, charset: &Charset) -> Vec<String> {
    render_cells(image, Mode::Ascii.cell_size(), |row, x, y| {
        match opaque(image, x, y) {
            Some(colour) => push_cell(row, colour, charset.pick(colour)),
            None => row.push(' '),
        }
    })
}

/// Renders `image` with one cell per 2×4 pixels: a braille character raising
/// the dot of each opaque pixel, in their rounded mean colour.
pub fn braille(image: &RgbaImage) -> Vec<String> {
    render_cells(image, Mode::Braille.cell_size(), |row, x, y| {
        let mut bits = 0;
        let mut count = 0;
        let mut sums = [0u32; 3];
        for (dx, dy, bit) in BRAILLE_DOTS {
            if let Some(colour) = opaque(image, x + dx, y + dy) {
                bits |= bit;
                count += 1;
                for (sum, channel) in sums.iter_mut().zip(colour) {
                    *sum += u32::from(channel);
                }
            }
        }
        if count == 0 {
            row.push(' ');
            return;
        }
        // Rounds half up: ⌊(2 × sum + n) ÷ 2n⌋.
        let mean = sums.map(|sum| {
            u8::try_from((2 * sum + count) / (2 * count)).expect("a mean of u8 values fits in u8")
        });
        let glyph = char::from_u32(BRAILLE_BLANK + bits).expect("U+2800..=U+28FF are characters");
        push_cell(row, mean, glyph);
    })
}

/// Builds one row per `cell` pixels down the image and calls `draw` for each
/// cell along it with the row so far and the cell's top-left pixel.
fn render_cells(
    image: &RgbaImage,
    (cell_width, cell_height): (u32, u32),
    mut draw: impl FnMut(&mut String, u32, u32),
) -> Vec<String> {
    (0..image.height().div_ceil(cell_height))
        .map(|cell_y| {
            let mut row = String::new();
            for cell_x in 0..image.width().div_ceil(cell_width) {
                draw(&mut row, cell_x * cell_width, cell_y * cell_height);
            }
            row
        })
        .collect()
}

/// The colour of the pixel at (`x`, `y`), or `None` when it is transparent or
/// outside the image.
fn opaque(image: &RgbaImage, x: u32, y: u32) -> Option<Rgb> {
    let [r, g, b, a] = image.get_pixel_checked(x, y)?.0;
    (a != 0).then_some([r, g, b])
}

/// Appends `glyph` in the foreground `colour`, then a reset.
fn push_cell(row: &mut String, colour: Rgb, glyph: char) {
    ansi::push_foreground(row, colour);
    row.push(glyph);
    row.push_str(RESET);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_mode_has_its_name() {
        assert_eq!(Mode::names(), "halfblock, ascii, braille");
    }
}
