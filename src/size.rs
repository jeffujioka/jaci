//! Art sizing: the columns and rows an image takes as art, and the image
//! resampled to the render mode's pixel grid for that many cells.

use std::borrow::Cow;

use image::imageops::{self, FilterType};
use image::{Rgba32FImage, RgbaImage};

use crate::render::Mode;

const DEFAULT_COLUMNS: u32 = 80;
const DEFAULT_RATIO: f64 = 0.5;
/// Lanczos3 keeps photos sharp when they shrink to a few dozen cells.
const FILTER: FilterType = FilterType::Lanczos3;

/// The sizing inputs, already valid: options from the command line and keys
/// from the config. `None` and `false` mean not given.
#[derive(Debug, Clone, Default, PartialEq)]
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
#[derive(Debug, Clone, PartialEq)]
pub struct Fit<'a> {
    pub columns: u32,
    pub rows: u32,
    /// `columns` × cell width by `rows` × cell height pixels; the input image
    /// itself when it already has that size.
    pub image: Cow<'a, RgbaImage>,
}

/// Fits `image` to art in `mode`. The columns come from the first of
/// `--full`, `--auto`, `-w`, `-s`, `[encode].width`, then 80; the rows are
/// round(columns × height ÷ width × ratio), at least 1, with the ratio from
/// `--ratio`, else `[encode].ratio`, else 0.5.
pub fn fit<'a>(image: &'a RgbaImage, mode: Mode, sizing: &Sizing) -> Fit<'a> {
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

    let target = (columns * cell_width, rows * cell_height);
    let image = if image.dimensions() == target {
        Cow::Borrowed(image)
    } else {
        Cow::Owned(resample(image, target.0, target.1))
    };
    Fit {
        columns,
        rows,
        image,
    }
}

/// `image` resized to `width` × `height` with its colour premultiplied by
/// alpha, as `imageops::resize` assumes, so the hidden colour of transparent
/// pixels never bleeds into their neighbours. A pixel that comes out with
/// alpha 0 is transparent black.
fn resample(image: &RgbaImage, width: u32, height: u32) -> RgbaImage {
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
    let resized = imageops::resize(&premultiplied, width, height, FILTER);
    let straight = resized
        .as_raw()
        .chunks_exact(4)
        .flat_map(|pixel| {
            let (alpha, a) = (byte(pixel[3]), pixel[3]);
            if alpha == 0 {
                return [0; 4];
            }
            [
                byte(pixel[0] / a),
                byte(pixel[1] / a),
                byte(pixel[2] / a),
                alpha,
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
fn rounded(value: f64) -> u32 {
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

        let fit = fit(&image, Mode::Halfblock, &Sizing::default());

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

        assert_eq!(fit(&image, Mode::Halfblock, &config).rows, 20);
        assert_eq!(fit(&image, Mode::Halfblock, &option).rows, 80);
    }
}
