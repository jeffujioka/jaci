//! Art sizing: the columns and rows an image takes as art, and the image
//! resampled to the render mode's pixel grid for that many cells.

use std::borrow::Cow;
use std::error::Error;
use std::fmt;

use image::imageops::{self, FilterType};
use image::{Rgba32FImage, RgbaImage};

use crate::render::Mode;

pub(crate) const DEFAULT_COLUMNS: u32 = 80;
pub(crate) const DEFAULT_RATIO: f64 = 0.5;
/// Lanczos3 keeps photos sharp when they shrink to a few dozen cells.
const FILTER: FilterType = FilterType::Lanczos3;
/// Decides which resampled pixels are transparent, and colours those where
/// [`FILTER`]'s alpha falls below its own and short of opaque: its kernel is
/// never negative, so it rings no alpha past a shape's edge, mixes no colour
/// the source lacks, and every source pixel whose weighted alpha rounds to
/// at least 1 reaches the output pixels around it.
const SMOOTH_FILTER: FilterType = FilterType::Triangle;
/// How far, relative to [`SMOOTH_FILTER`]'s alpha, [`FILTER`]'s alpha may sit
/// below it and still count as equal: the two filters' weights sum to 1 in
/// different float rounding, so a uniform alpha comes out a few `f32` steps
/// apart from each.
const ALPHA_TOLERANCE: f32 = 1e-5;
/// The most pixels a fitted image may hold: 2^25. It bounds the output grid
/// only, 512 MiB for each of the resample's two `f32` RGBA results; the
/// source's premultiplied `f32` copy and the intermediate buffers of
/// `imageops::resize` (source width × target height) grow with the source
/// and are not bounded by it.
const MAX_PIXELS: u32 = 1 << 25;
/// The blur radius of the unsharp mask [`fit`] applies in quadrant mode.
const SHARPEN_SIGMA: f32 = 1.0;
/// The least difference between a channel and its blurred value, of 255,
/// that the unsharp mask amplifies.
const SHARPEN_THRESHOLD: i32 = 2;

/// Whether `ratio` is a valid character width ÷ height: finite and above 0.
pub fn valid_ratio(ratio: f64) -> bool {
    ratio.is_finite() && ratio > 0.0
}

/// The sizing inputs, already valid: options from the command line and keys
/// from the config. `None` and `false` mean not given.
#[derive(Debug, Clone, Default)]
pub struct Sizing {
    /// `--full`: one column per cell of image pixels across.
    pub full: bool,
    /// The terminal's width in columns when `--auto` is given.
    pub auto: Option<u16>,
    /// `-w <columns>`.
    pub width: Option<u32>,
    /// `-s <percent>`: that percent of the `--full` width.
    pub scale: Option<u32>,
    /// `[encode].width`.
    pub config_width: Option<u32>,
    /// `--ratio`: character width ÷ height.
    pub ratio: Option<f64>,
    /// `[encode].ratio`.
    pub config_ratio: Option<f64>,
}

/// An image fitted to art: its size in cells and the image at the render
/// mode's pixel grid for that size.
#[derive(Debug)]
pub struct Fit<'a> {
    pub columns: u32,
    pub rows: u32,
    /// `columns` × cell width by `rows` × cell height pixels; the input image
    /// itself when it already has that size.
    pub image: Cow<'a, RgbaImage>,
}

/// Why an image cannot be fitted: its art's pixel grid would hold more than
/// 2^25 pixels.
#[derive(Debug)]
pub struct TooLarge {
    pub columns: u32,
    pub rows: u32,
}

impl fmt::Display for TooLarge {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "jaci: art of {}×{} cells is too large",
            self.columns, self.rows
        )
    }
}

impl Error for TooLarge {}

/// Fits `image` to art in `mode`. The columns come from the first of
/// `--full`, `--auto`, `-w`, `-s`, `[encode].width`, then 80; the rows are
/// round(columns × height ÷ width × ratio), at least 1, with the ratio from
/// `--ratio`, else `[encode].ratio`, else 0.5. An image that already has the
/// art's pixel grid is used as it is; any other is resampled to it, and in
/// quadrant mode a resampled image whose every pixel is opaque is then
/// sharpened with an unsharp mask. An image with any transparency is not
/// sharpened: that would smear the hidden colour of its transparent pixels
/// into the edges.
///
/// # Errors
///
/// [`TooLarge`] when the art's pixel grid would hold more than 2^25
/// pixels.
pub fn fit<'a>(image: &'a RgbaImage, mode: Mode, sizing: &Sizing) -> Result<Fit<'a>, TooLarge> {
    let (width, height) = image.dimensions();
    let (cell_width, cell_height) = mode.cell_size();
    let full = rounded(f64::from(width) / f64::from(cell_width));
    let columns = if sizing.full {
        full
    } else if let Some(terminal) = sizing.auto {
        u32::from(terminal)
    } else if let Some(columns) = sizing.width {
        columns
    } else if let Some(percent) = sizing.scale {
        rounded(f64::from(full) * f64::from(percent) / 100.0)
    } else {
        sizing.config_width.unwrap_or(DEFAULT_COLUMNS)
    };
    let ratio = sizing
        .ratio
        .or(sizing.config_ratio)
        .unwrap_or(DEFAULT_RATIO);
    let rows = rounded(f64::from(columns) * f64::from(height) / f64::from(width) * ratio);

    let target = columns
        .checked_mul(cell_width)
        .zip(rows.checked_mul(cell_height))
        .filter(|&(width, height)| {
            width
                .checked_mul(height)
                .is_some_and(|pixels| pixels <= MAX_PIXELS)
        })
        .ok_or(TooLarge { columns, rows })?;
    let image = if image.dimensions() == target {
        Cow::Borrowed(image)
    } else {
        let resampled = resample(image, target.0, target.1);
        Cow::Owned(if mode == Mode::Quadrant && opaque(&resampled) {
            imageops::unsharpen(&resampled, SHARPEN_SIGMA, SHARPEN_THRESHOLD)
        } else {
            resampled
        })
    };
    Ok(Fit {
        columns,
        rows,
        image,
    })
}

/// Whether every pixel of `image` has alpha 255.
fn opaque(image: &RgbaImage) -> bool {
    image.pixels().all(|pixel| pixel.0[3] == u8::MAX)
}

/// `image` resized to `width` × `height`, its colour premultiplied by alpha,
/// as `imageops::resize` assumes, so the hidden colour of transparent pixels
/// never bleeds into their neighbours. The image is resized twice, with
/// [`FILTER`] and with [`SMOOTH_FILTER`]. A pixel whose [`SMOOTH_FILTER`]
/// alpha rounds to 0 is transparent black, so [`FILTER`]'s ringing past a
/// shape's edge draws no cells. A pixel whose [`FILTER`] alpha is below its
/// [`SMOOTH_FILTER`] alpha by more than [`ALPHA_TOLERANCE`] of it and rounds
/// below 255, the side of an edge where [`FILTER`] undershoots, takes its
/// colour and alpha from [`SMOOTH_FILTER`]: there [`FILTER`]'s alpha is small
/// or not above 0, so dividing by it would push the colour beside an outline
/// past every source colour, or leave it black. [`SMOOTH_FILTER`]'s colour is
/// a mix of the source's own colours, and its pixel stays opaque to the
/// render (D28) however translucent or thin its source. Every other pixel
/// keeps [`FILTER`]'s sharper colour and alpha: where [`FILTER`] matches,
/// up to float rounding, or overshoots, as in a uniformly translucent image,
/// and where its alpha rounds to 255, as in an opaque image, whose resampled
/// alphas sit on or a float rounding step below 1 and whose colour that
/// alpha cannot distort.
pub(crate) fn resample(image: &RgbaImage, width: u32, height: u32) -> RgbaImage {
    let premultiplied = image
        .as_raw()
        .chunks_exact(4)
        .flat_map(|pixel| {
            let [r, g, b, a] = [pixel[0], pixel[1], pixel[2], pixel[3]].map(unit);
            [r * a, g * a, b * a, a]
        })
        .collect();
    let premultiplied = Rgba32FImage::from_raw(image.width(), image.height(), premultiplied)
        .expect("the buffer holds four channels per pixel");
    let sharp = imageops::resize(&premultiplied, width, height, FILTER);
    let smooth = imageops::resize(&premultiplied, width, height, SMOOTH_FILTER);
    let straight = sharp
        .as_raw()
        .chunks_exact(4)
        .zip(smooth.as_raw().chunks_exact(4))
        .flat_map(|(sharp, smooth)| {
            if byte(smooth[3]) == 0 {
                return [0; 4];
            }
            // Above 0 either way: `smooth` is at least the half that rounds
            // to a byte of 1, and `sharp` is kept only when at least that,
            // less a float rounding step, or when it rounds to 255.
            let undershoots = sharp[3] < smooth[3] * (1.0 - ALPHA_TOLERANCE);
            let pixel = if undershoots && byte(sharp[3]) < 255 {
                smooth
            } else {
                sharp
            };
            let a = pixel[3];
            [
                byte(pixel[0] / a),
                byte(pixel[1] / a),
                byte(pixel[2] / a),
                byte(a),
            ]
        })
        .collect();
    RgbaImage::from_raw(width, height, straight).expect("the buffer holds four channels per pixel")
}

/// `channel` scaled from 0..=255 to 0.0..=1.0.
fn unit(channel: u8) -> f32 {
    f32::from(channel) / 255.0
}

/// `value` clamped to 0.0..=1.0 and scaled to the nearest of 0..=255.
fn byte(value: f32) -> u8 {
    // In range after the clamp, so the cast is exact.
    (value.clamp(0.0, 1.0) * 255.0).round() as u8
}

/// `value` rounded half away from zero, at least 1.
pub(crate) fn rounded(value: f64) -> u32 {
    // The cast saturates, so a value beyond u32 stays in range.
    (value.round() as u32).max(1)
}

#[cfg(test)]
mod tests {
    use image::Rgba;

    use super::*;

    #[test]
    fn image_at_target_size_is_used_unchanged() {
        let image = RgbaImage::from_pixel(80, 80, Rgba([1, 2, 3, 255]));

        let fit = fit(&image, Mode::Halfblock, &Sizing::default()).unwrap();

        assert!(matches!(fit.image, Cow::Borrowed(borrowed) if std::ptr::eq(borrowed, &image)));
    }

    #[test]
    fn config_ratio_applies_and_option_beats_it() {
        let image = RgbaImage::from_pixel(100, 100, Rgba([1, 2, 3, 255]));
        let config = Sizing {
            config_ratio: Some(0.25),
            ..Sizing::default()
        };
        let option = Sizing {
            ratio: Some(1.0),
            ..config.clone()
        };

        assert_eq!(fit(&image, Mode::Halfblock, &config).unwrap().rows, 20);
        assert_eq!(fit(&image, Mode::Halfblock, &option).unwrap().rows, 80);
    }

    #[test]
    fn ratio_is_valid_only_finite_and_above_zero() {
        for ratio in [0.5, 1.0, f64::MIN_POSITIVE, 1e300] {
            assert!(valid_ratio(ratio), "{ratio}");
        }
        for ratio in [0.0, -0.0, -1.0, f64::INFINITY, f64::NEG_INFINITY, f64::NAN] {
            assert!(!valid_ratio(ratio), "{ratio}");
        }
    }
}
