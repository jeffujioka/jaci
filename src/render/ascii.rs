//! Ascii mode: one character per pixel, picked by the pixel's luminance, in
//! one of four styles.

use std::fmt::{self, Write as _};

use image::RgbaImage;

use super::{Mode, Rgb, opaque, push_cell, render_cells};
use crate::ansi::{self, RESET};
use crate::named::Named;

pub(crate) const DEFAULT_CHARSET: &str = " .:-=+*#%@";
const DENSE_CHARSET: &str =
    " .'`^\",:;Il!i><~+_-?][}{1)(|\\/tfjrxnuvczXYUJCLQ0OZmwqpdbkhao*#MW&8%B@$";
/// The luminance of white, in thousandths.
const WHITE_LUMINANCE: f64 = 255_000.0;
/// The brightness the darkest character is drawn at.
const DARKEST_VALUE: f64 = 0.6;
/// How much brighter than the darkest the brightest character is drawn.
const VALUE_SPAN: f64 = 0.4;
/// The share of a pixel's colour the tinted style puts behind its character.
const TINT: f64 = 0.4;

/// How ascii mode picks and colours its characters.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AsciiStyle {
    /// The character the pixel's luminance picks, in the pixel's colour.
    Plain,
    /// Contrast stretched between the image's 2nd and 98th luminance
    /// percentile, and each character in its pixel's hue at a brightness
    /// lifted with its level.
    #[default]
    Bright,
    /// [`Bright`](Self::Bright) with a charset of 70 characters.
    Dense,
    /// [`Bright`](Self::Bright) over the pixel's colour at 40 % as
    /// background.
    Tinted,
}

impl Named for AsciiStyle {
    /// In the order the help and the messages list them.
    const ALL: &'static [Self] = &[Self::Plain, Self::Bright, Self::Dense, Self::Tinted];

    fn name(self) -> &'static str {
        match self {
            Self::Plain => "plain",
            Self::Bright => "bright",
            Self::Dense => "dense",
            Self::Tinted => "tinted",
        }
    }
}

impl AsciiStyle {
    /// The characters the style draws with when none are chosen.
    pub fn charset(self) -> Charset {
        match self {
            Self::Dense => Charset(DENSE_CHARSET.chars().collect()),
            Self::Plain | Self::Bright | Self::Tinted => Charset::default(),
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
    fn pick(&self, colour: Rgb) -> char {
        // L is at most 255, below 256, so the index stays below n.
        let index = u64::from(luminance(colour)) * self.0.len() as u64 / 256_000;
        self.0[usize::try_from(index).expect("the index is below the charset length")]
    }

    /// The character at index ⌊`level` × n⌋, with n the charset length and
    /// `level` from 0 to 1; a level of 1 gives the last character.
    fn at_level(&self, level: f64) -> char {
        let last = self.0.len() - 1;
        self.0[((level * self.0.len() as f64) as usize).min(last)]
    }
}

impl Default for Charset {
    /// ` .:-=+*#%@`
    fn default() -> Self {
        Self(DEFAULT_CHARSET.chars().collect())
    }
}

impl fmt::Display for Charset {
    /// The characters, darkest first.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.iter().try_for_each(|&c| f.write_char(c))
    }
}

/// Renders `image` with one cell per pixel: the `charset` character its
/// luminance picks, coloured as `style` says.
pub fn ascii(image: &RgbaImage, style: AsciiStyle, charset: &Charset) -> Vec<String> {
    let cell_size = Mode::Ascii.cell_size();
    if style == AsciiStyle::Plain {
        return render_cells(image, cell_size, |row, x, y| match opaque(image, x, y) {
            Some(colour) => push_cell(row, colour, charset.pick(colour)),
            None => row.push(' '),
        });
    }
    let range = luminance_range(image);
    render_cells(image, cell_size, |row, x, y| {
        // There is a range whenever there is an opaque pixel.
        let (Some(colour), Some(range)) = (opaque(image, x, y), range) else {
            row.push(' ');
            return;
        };
        let level = level(luminance(colour), range);
        ansi::push_foreground(row, lifted(colour, level));
        if style == AsciiStyle::Tinted {
            ansi::push_background(
                row,
                colour.map(|channel| to_channel(f64::from(channel) * TINT)),
            );
        }
        row.push(charset.at_level(level));
        row.push_str(RESET);
    })
}

/// 299R + 587G + 114B: the luminance of `colour` in thousandths.
fn luminance([r, g, b]: Rgb) -> u32 {
    299 * u32::from(r) + 587 * u32::from(g) + 114 * u32::from(b)
}

/// The 2nd and 98th percentile of the luminance of `image`'s opaque pixels,
/// or `None` when it has none.
fn luminance_range(image: &RgbaImage) -> Option<(u32, u32)> {
    let mut sorted: Vec<u32> = image
        .pixels()
        .filter(|pixel| pixel.0[3] != 0)
        .map(|pixel| luminance([pixel.0[0], pixel.0[1], pixel.0[2]]))
        .collect();
    if sorted.is_empty() {
        return None;
    }
    sorted.sort_unstable();
    let count = sorted.len();
    Some((sorted[2 * count / 100], sorted[98 * count / 100]))
}

/// How bright `luminance` is from 0 to 1: its place between `lo` and `hi`,
/// or between black and white when they are equal, raised to the power ¾.
fn level(luminance: u32, (lo, hi): (u32, u32)) -> f64 {
    let luminance = f64::from(luminance);
    let t = if hi == lo {
        luminance / WHITE_LUMINANCE
    } else {
        ((luminance - f64::from(lo)) / f64::from(hi - lo)).clamp(0.0, 1.0)
    };
    // Two square roots, not `powf`, which differs between platforms.
    (t * t * t).sqrt().sqrt()
}

/// `colour`'s hue at the brightness `level` lifts it to: grey for black.
fn lifted(colour: Rgb, level: f64) -> Rgb {
    let value = DARKEST_VALUE + VALUE_SPAN * level;
    match colour.into_iter().max() {
        Some(max) if max != 0 => {
            colour.map(|channel| to_channel(f64::from(channel) / f64::from(max) * value * 255.0))
        }
        _ => [to_channel(value * 255.0); 3],
    }
}

/// `value`, from 0 to 255, rounded half away from zero.
fn to_channel(value: f64) -> u8 {
    value.round() as u8
}
