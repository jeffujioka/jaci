use image::{Rgba, RgbaImage};
use jaci::render::Mode;
use jaci::size::{self, Sizing};

/// An opaque grey image of `width` × `height` pixels.
fn image(width: u32, height: u32) -> RgbaImage {
    RgbaImage::from_pixel(width, height, Rgba([128, 128, 128, 255]))
}

/// The columns and rows of `image` fitted in `mode` with `sizing`.
fn cells(image: &RgbaImage, mode: Mode, sizing: &Sizing) -> (u32, u32) {
    let fit = size::fit(image, mode, sizing);
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

    let fit = size::fit(&image, Mode::Halfblock, &Sizing::default());

    assert_eq!(fit.rows, 40);
    assert_eq!(fit.image.dimensions(), (80, 80));
}

#[test]
fn pixel_grid_follows_mode() {
    let image = image(100, 100);

    let braille = size::fit(&image, Mode::Braille, &Sizing::default());
    let ascii = size::fit(&image, Mode::Ascii, &Sizing::default());

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

    let fit = size::fit(&image, Mode::Halfblock, &sizing);

    assert_eq!(fit.image.dimensions(), (4, 4));
    for pixel in fit.image.pixels().filter(|pixel| pixel[3] != 0) {
        assert!(pixel[0] <= 2, "red bled into {pixel:?}");
    }
}
