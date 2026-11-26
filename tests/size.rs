use image::imageops::{self, FilterType};
use image::{Rgba, Rgba32FImage, RgbaImage};
use jaci::render::Mode;
use jaci::size::{self, Sizing};

/// An opaque grey image of `width` × `height` pixels.
fn image(width: u32, height: u32) -> RgbaImage {
    RgbaImage::from_pixel(width, height, Rgba([128, 128, 128, 255]))
}

/// The columns and rows of `image` fitted in `mode` with `sizing`.
fn cells(image: &RgbaImage, mode: Mode, sizing: &Sizing) -> (u32, u32) {
    let fit = size::fit(image, mode, sizing).unwrap();
    (fit.columns, fit.rows)
}

/// The columns of a 200×100 image fitted in halfblock mode with `sizing`.
fn columns(sizing: &Sizing) -> u32 {
    cells(&image(200, 100), Mode::Halfblock, sizing).0
}

#[test]
fn default_width_is_80() {
    assert_eq!(columns(&Sizing::default()), 80);
}

#[test]
fn width_option_sets_columns() {
    let sizing = Sizing {
        width: Some(30),
        ..Sizing::default()
    };

    assert_eq!(columns(&sizing), 30);
}

#[test]
fn scale_is_percentage_of_full_width() {
    let sizing = Sizing {
        scale: Some(50),
        ..Sizing::default()
    };

    assert_eq!(columns(&sizing), 100);
}

#[test]
fn full_width_follows_pixels() {
    let sizing = Sizing {
        full: true,
        ..Sizing::default()
    };

    assert_eq!(cells(&image(200, 100), Mode::Braille, &sizing).0, 100);
}

#[test]
fn auto_width_takes_terminal_width() {
    let sizing = Sizing {
        auto: Some(100),
        ..Sizing::default()
    };

    assert_eq!(columns(&sizing), 100);
}

#[test]
fn full_beats_auto_beats_width() {
    let auto = Sizing {
        auto: Some(100),
        width: Some(10),
        ..Sizing::default()
    };
    let full = Sizing {
        full: true,
        ..auto.clone()
    };

    assert_eq!(columns(&full), 200);
    assert_eq!(columns(&auto), 100);
}

#[test]
fn config_width_applies() {
    let sizing = Sizing {
        config_width: Some(50),
        ..Sizing::default()
    };

    assert_eq!(columns(&sizing), 50);
}

#[test]
fn square_image_in_halfblock() {
    let image = image(100, 100);

    let fit = size::fit(&image, Mode::Halfblock, &Sizing::default()).unwrap();

    assert_eq!(fit.rows, 40);
    assert_eq!(fit.image.dimensions(), (80, 80));
}

#[test]
fn pixel_grid_follows_mode() {
    let image = image(100, 100);

    let braille = size::fit(&image, Mode::Braille, &Sizing::default()).unwrap();
    let ascii = size::fit(&image, Mode::Ascii, &Sizing::default()).unwrap();

    assert_eq!((braille.columns, braille.rows), (80, 40));
    assert_eq!(braille.image.dimensions(), (160, 160));
    assert_eq!((ascii.columns, ascii.rows), (80, 40));
    assert_eq!(ascii.image.dimensions(), (80, 40));
}

#[test]
fn ratio_changes_rows() {
    let sizing = Sizing {
        ratio: Some(1.0),
        ..Sizing::default()
    };

    assert_eq!(cells(&image(100, 100), Mode::Halfblock, &sizing).1, 80);
}

#[test]
fn at_least_one_row() {
    assert_eq!(
        cells(&image(1000, 1), Mode::Halfblock, &Sizing::default()).1,
        1
    );
}

#[test]
fn transparent_colour_does_not_bleed() {
    let red_transparent = Rgba([255, 0, 0, 0]);
    let blue = Rgba([0, 0, 255, 255]);
    let image = RgbaImage::from_fn(8, 8, |x, _| if x < 4 { red_transparent } else { blue });
    let sizing = Sizing {
        width: Some(4),
        ..Sizing::default()
    };

    let fit = size::fit(&image, Mode::Halfblock, &sizing).unwrap();

    assert_eq!(fit.image.dimensions(), (4, 4));
    for pixel in fit.image.pixels().filter(|pixel| pixel[3] != 0) {
        assert!(pixel[0] <= 2, "red bled into {pixel:?}");
    }
}

#[test]
fn resampling_rings_no_alpha_around_a_shape() {
    // An opaque 32×32 square in the middle of a transparent 64×64 image.
    let image = RgbaImage::from_fn(64, 64, |x, y| {
        if (16..48).contains(&x) && (16..48).contains(&y) {
            Rgba([0, 0, 255, 255])
        } else {
            Rgba([0, 0, 0, 0])
        }
    });
    let sizing = Sizing {
        width: Some(16),
        ..Sizing::default()
    };

    let fit = size::fit(&image, Mode::Halfblock, &sizing).unwrap();

    // At a quarter of the size the square covers 4..12; one pixel beyond it
    // may blend with the edge, anything further must stay transparent.
    assert_eq!(fit.image.dimensions(), (16, 16));
    let near = |coordinate| (3..13).contains(&coordinate);
    let rings: Vec<_> = fit
        .image
        .enumerate_pixels()
        .filter(|&(x, y, pixel)| !(near(x) && near(y)) && pixel[3] != 0)
        .collect();
    assert!(rings.is_empty(), "{} pixels: {rings:?}", rings.len());
}

#[test]
fn uniformly_translucent_image_stays_visible_when_resampled() {
    let image = RgbaImage::from_pixel(64, 64, Rgba([0, 0, 255, 100]));
    let sizing = Sizing {
        width: Some(16),
        ..Sizing::default()
    };

    let fit = size::fit(&image, Mode::Halfblock, &sizing).unwrap();

    let blank: Vec<_> = fit
        .image
        .enumerate_pixels()
        .filter(|&(_, _, pixel)| pixel[3] == 0)
        .collect();
    assert!(blank.is_empty(), "{} pixels: {blank:?}", blank.len());
}

#[test]
fn thin_stroke_survives_resampling() {
    // A 1-pixel-wide opaque vertical line down a transparent 64×64 image.
    let image = RgbaImage::from_fn(64, 64, |x, _| {
        if x == 32 {
            Rgba([255, 255, 255, 255])
        } else {
            Rgba([0, 0, 0, 0])
        }
    });
    let sizing = Sizing {
        width: Some(16),
        ..Sizing::default()
    };

    let fit = size::fit(&image, Mode::Halfblock, &sizing).unwrap();

    let empty_rows: Vec<_> = (0..fit.image.height())
        .filter(|&y| (0..fit.image.width()).all(|x| fit.image.get_pixel(x, y)[3] == 0))
        .collect();
    assert!(
        empty_rows.is_empty(),
        "rows without the line: {empty_rows:?}"
    );
}

#[test]
fn shape_edges_draw_no_dark_fringe_at_any_alignment() {
    // An opaque blue square over s..58 of a transparent 80×80 image, shrunk
    // 5×: sweeping s moves its edge across every phase of an output pixel.
    let sizing = Sizing {
        width: Some(16),
        ..Sizing::default()
    };
    let fringe: Vec<_> = (16..=24)
        .flat_map(|start| {
            let image = RgbaImage::from_fn(80, 80, |x, y| {
                if (start..58).contains(&x) && (start..58).contains(&y) {
                    Rgba([0, 0, 255, 255])
                } else {
                    Rgba([0, 0, 0, 0])
                }
            });
            let fit = size::fit(&image, Mode::Halfblock, &sizing).unwrap();
            assert_eq!(fit.image.dimensions(), (16, 16));
            fit.image
                .enumerate_pixels()
                .filter(|&(_, _, pixel)| {
                    pixel[3] != 0 && (pixel[2] < 128 || pixel[0] > 2 || pixel[1] > 2)
                })
                .map(|(x, y, pixel)| (start, x, y, *pixel))
                .collect::<Vec<_>>()
        })
        .collect();
    assert!(
        fringe.is_empty(),
        "{} pixels (start, x, y, pixel): {fringe:?}",
        fringe.len()
    );
}

/// An opaque square over `start..58` of a transparent 80×80 image, `fill`
/// inside an `outline` band `width` pixels wide along its outer edge.
fn outlined_square(start: u32, width: u32, fill: [u8; 3], outline: [u8; 3]) -> RgbaImage {
    let inside = |coordinate| (start..58).contains(&coordinate);
    let band = |coordinate| coordinate < start + width || coordinate >= 58 - width;
    RgbaImage::from_fn(80, 80, |x, y| {
        if !(inside(x) && inside(y)) {
            Rgba([0, 0, 0, 0])
        } else if band(x) || band(y) {
            Rgba([outline[0], outline[1], outline[2], 255])
        } else {
            Rgba([fill[0], fill[1], fill[2], 255])
        }
    })
}

/// `image` resized to `width` × `height` with a Triangle filter, its colour
/// premultiplied by alpha: a convex mix of the source's pixels, computed
/// with the `image` crate alone. Each pixel is its straight colour and alpha
/// in 0.0..=1.0.
fn triangle_resample(image: &RgbaImage, width: u32, height: u32) -> Vec<[f32; 4]> {
    let premultiplied = image
        .as_raw()
        .chunks_exact(4)
        .flat_map(|pixel| {
            let a = f32::from(pixel[3]) / 255.0;
            [0, 1, 2]
                .map(|channel| f32::from(pixel[channel]) / 255.0 * a)
                .into_iter()
                .chain([a])
        })
        .collect();
    let premultiplied = Rgba32FImage::from_raw(image.width(), image.height(), premultiplied)
        .expect("the buffer holds four channels per pixel");
    imageops::resize(&premultiplied, width, height, FilterType::Triangle)
        .pixels()
        .map(|&Rgba([r, g, b, a])| {
            if a > 0.0 {
                [r / a, g / a, b / a, a]
            } else {
                [0.0; 4]
            }
        })
        .collect()
}

#[test]
fn two_colour_edges_keep_colours_between_their_sources() {
    // Every visible output pixel must stay near the convex colour a Triangle
    // resample gives it: an outline must not push the colour beside it past
    // both of its source colours.
    const PAIRS: [([u8; 3], [u8; 3]); 2] = [
        ([255, 255, 255], [0, 0, 0]),
        ([32, 32, 32], [255, 255, 255]),
    ];
    let mut off = Vec::new();
    for (fill, outline) in PAIRS {
        for outline_width in 1..=6 {
            for columns in [16, 20] {
                for start in 16..=24 {
                    let image = outlined_square(start, outline_width, fill, outline);
                    let sizing = Sizing {
                        width: Some(columns),
                        ..Sizing::default()
                    };
                    let fit = size::fit(&image, Mode::Halfblock, &sizing).unwrap();
                    let (width, height) = fit.image.dimensions();
                    let oracle = triangle_resample(&image, width, height);
                    off.extend(
                        fit.image
                            .pixels()
                            .zip(&oracle)
                            .enumerate()
                            .filter(|&(_, (pixel, convex))| {
                                pixel[3] != 0
                                    && (0..3).any(|channel| {
                                        let expected = (convex[channel] * 255.0).round();
                                        (f32::from(pixel[channel]) - expected).abs() > 128.0
                                    })
                            })
                            .map(|(index, (pixel, _))| {
                                (fill, outline_width, columns, start, index, *pixel)
                            }),
                    );
                }
            }
        }
    }
    assert!(
        off.is_empty(),
        "{} pixels (fill, outline width, columns, start, index, pixel): {off:?}",
        off.len()
    );
}

/// The pixels of `image`'s fit in halfblock mode at `columns` whose
/// channels differ by more than 1 from a plain Lanczos3 resize of `image`
/// premultiplied by alpha, divided by its alpha after, computed with the
/// `image` crate alone, as (index, fitted, reference).
fn off_lanczos(image: &RgbaImage, columns: u32) -> Vec<(usize, Rgba<u8>, [u8; 4])> {
    let sizing = Sizing {
        width: Some(columns),
        ..Sizing::default()
    };
    let fit = size::fit(image, Mode::Halfblock, &sizing).unwrap();
    let (width, height) = fit.image.dimensions();
    let source = image
        .as_raw()
        .chunks_exact(4)
        .flat_map(|pixel| {
            let a = f32::from(pixel[3]) / 255.0;
            [0, 1, 2]
                .map(|channel| f32::from(pixel[channel]) / 255.0 * a)
                .into_iter()
                .chain([a])
        })
        .collect();
    let source = Rgba32FImage::from_raw(image.width(), image.height(), source)
        .expect("the buffer holds four channels per pixel");
    let reference = imageops::resize(&source, width, height, FilterType::Lanczos3);
    fit.image
        .pixels()
        .zip(reference.pixels())
        .enumerate()
        .filter_map(|(index, (pixel, &Rgba([r, g, b, a])))| {
            let expected = [r / a, g / a, b / a, a]
                .map(|channel| (channel.clamp(0.0, 1.0) * 255.0).round() as u8);
            (0..4)
                .any(|channel| pixel[channel].abs_diff(expected[channel]) > 1)
                .then_some((index, *pixel, expected))
        })
        .collect()
}

/// `side`×`side` black and white vertical stripes `side / 16` pixels wide,
/// every pixel of alpha `alpha`.
fn stripes(side: u32, alpha: u8) -> RgbaImage {
    RgbaImage::from_fn(side, side, |x, _| {
        let level = if (x / (side / 16)).is_multiple_of(2) {
            0
        } else {
            255
        };
        Rgba([level, level, level, alpha])
    })
}

#[test]
fn opaque_stripes_keep_lanczos_sharpness() {
    for columns in [16, 80] {
        let off = off_lanczos(&stripes(64, 255), columns);
        assert!(
            off.is_empty(),
            "-w {columns}: {} pixels (index, fitted, reference): {off:?}",
            off.len()
        );
    }
}

#[test]
fn translucent_stripes_keep_lanczos_sharpness() {
    for columns in [16, 80] {
        let off = off_lanczos(&stripes(64, 128), columns);
        assert!(
            off.is_empty(),
            "-w {columns}: {} pixels (index, fitted, reference): {off:?}",
            off.len()
        );
    }
}

#[test]
fn translucent_stripes_keep_lanczos_sharpness_at_a_large_shrink() {
    // A 64× shrink: each output pixel sums hundreds of Lanczos3 taps, so the
    // two filters' alphas drift furthest apart in float rounding.
    let off = off_lanczos(&stripes(1024, 128), 16);
    assert!(
        off.is_empty(),
        "{} pixels (index, fitted, reference): {off:?}",
        off.len()
    );
}

#[test]
fn opaque_gradient_with_hard_edge_keeps_lanczos_sharpness() {
    // A fully opaque 200×133 image: a smooth gradient on one side of a hard
    // diagonal edge and a dark flat colour on the other.
    let image = RgbaImage::from_fn(200, 133, |x, y| {
        if 2 * x > 3 * y {
            let red = (x * 255 / 199) as u8;
            let green = (y * 255 / 132) as u8;
            Rgba([red, green, 255 - red / 2, 255])
        } else {
            Rgba([20, 30, 40, 255])
        }
    });

    for columns in [16, 40, 80] {
        let off = off_lanczos(&image, columns);
        assert!(
            off.is_empty(),
            "-w {columns}: {} pixels (index, fitted, reference): {off:?}",
            off.len()
        );
    }
}
