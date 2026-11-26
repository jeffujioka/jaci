use std::fs;
use std::path::Path;

use image::{Rgba, RgbaImage};
use jaci::art_file;
use jaci::named::Named;
use jaci::render::{self, AsciiStyle, Charset};
use tempfile::TempDir;

const TRANSPARENT: Rgba<u8> = Rgba([0, 0, 0, 0]);

fn image(width: u32, height: u32, pixels: &[Rgba<u8>]) -> RgbaImage {
    let raw = pixels.iter().flat_map(|pixel| pixel.0).collect();
    RgbaImage::from_raw(width, height, raw).unwrap()
}

fn fixture() -> RgbaImage {
    image::open("test_fixtures/4x4.png").unwrap().to_rgba8()
}

/// Writes `rows` through the art file writer and checks the bytes and size
/// against the golden art file.
fn assert_golden(rows: &[String], golden: &str, columns: usize, lines: usize) {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("art.ansi");
    art_file::write(&path, rows).unwrap();
    let golden = Path::new("test_fixtures").join(golden);

    assert_eq!(
        fs::read_to_string(&path).unwrap(),
        fs::read_to_string(&golden).unwrap()
    );
    let art = art_file::read(&golden).unwrap();
    assert_eq!((art.columns(), art.rows().len()), (columns, lines));
}

#[test]
fn halfblock_two_opaque_pixels() {
    let image = image(1, 2, &[Rgba([255, 0, 0, 255]), Rgba([0, 0, 255, 255])]);

    assert_eq!(
        render::halfblock(&image),
        ["\x1b[38;2;255;0;0m\x1b[48;2;0;0;255m▀\x1b[0m"]
    );
}

#[test]
fn halfblock_transparent_top() {
    let image = image(1, 2, &[TRANSPARENT, Rgba([0, 255, 0, 255])]);

    assert_eq!(render::halfblock(&image), ["\x1b[38;2;0;255;0m▄\x1b[0m"]);
}

#[test]
fn halfblock_transparent_cell_is_space() {
    let image = image(1, 2, &[TRANSPARENT, TRANSPARENT]);

    assert_eq!(render::halfblock(&image), [" "]);
}

#[test]
fn halfblock_golden() {
    assert_golden(&render::halfblock(&fixture()), "4x4-halfblock.ansi", 4, 2);
}
#[test]
fn halfblock_missing_bottom_pixel_is_transparent() {
    let image = image(1, 1, &[Rgba([255, 0, 0, 255])]);

    assert_eq!(render::halfblock(&image), ["\x1b[38;2;255;0;0m▀\x1b[0m"]);
}

#[test]
fn ascii_luminance_picks_character() {
    let image = image(2, 1, &[Rgba([0, 0, 0, 255]), Rgba([255, 255, 255, 255])]);

    assert_eq!(
        render::ascii(&image, AsciiStyle::Plain, &Charset::default()),
        ["\x1b[38;2;0;0;0m \x1b[0m\x1b[38;2;255;255;255m@\x1b[0m"]
    );
}

#[test]
fn ascii_custom_charset() {
    let image = image(
        2,
        1,
        &[Rgba([100, 100, 100, 255]), Rgba([200, 200, 200, 255])],
    );
    let charset = Charset::new("ab").unwrap();

    assert_eq!(
        render::ascii(&image, AsciiStyle::Plain, &charset),
        ["\x1b[38;2;100;100;100ma\x1b[0m\x1b[38;2;200;200;200mb\x1b[0m"]
    );
}

#[test]
fn ascii_transparent_pixel_is_space() {
    let image = image(1, 1, &[TRANSPARENT]);

    assert_eq!(
        render::ascii(&image, AsciiStyle::Plain, &Charset::default()),
        [" "]
    );
}

#[test]
fn ascii_exact_luminance_boundary() {
    // L is exactly 128, so the index is ⌊128 × 10 ÷ 256⌋ = 5.
    let image = image(1, 1, &[Rgba([128, 128, 128, 255])]);

    assert_eq!(
        render::ascii(&image, AsciiStyle::Plain, &Charset::default()),
        ["\x1b[38;2;128;128;128m+\x1b[0m"]
    );
}

#[test]
fn ascii_golden() {
    assert_golden(
        &render::ascii(&fixture(), AsciiStyle::Plain, &Charset::default()),
        "4x4-ascii.ansi",
        4,
        4,
    );
}

/// One opaque pixel per colour, in a row.
fn row(colours: &[[u8; 3]]) -> RgbaImage {
    let pixels: Vec<_> = colours
        .iter()
        .map(|&[r, g, b]| Rgba([r, g, b, 255]))
        .collect();
    image(colours.len() as u32, 1, &pixels)
}

#[test]
fn ascii_styles_have_their_names_and_bright_is_the_default() {
    assert_eq!(AsciiStyle::names(), "plain, bright, dense, tinted");
    assert_eq!(AsciiStyle::default(), AsciiStyle::Bright);
}

#[test]
fn bright_stretches_contrast_and_lifts_brightness() {
    // lo = 0 and hi = 255 000, so the levels are 0 and 1.
    assert_eq!(
        render::ascii(
            &row(&[[0, 0, 0], [255, 255, 255]]),
            AsciiStyle::Bright,
            &Charset::default()
        ),
        ["\x1b[38;2;153;153;153m \x1b[0m\x1b[38;2;255;255;255m@\x1b[0m"]
    );
}

#[test]
fn bright_keeps_the_hue() {
    let image = row(&[[120, 60, 0], [240, 120, 0], [0, 0, 0], [255, 255, 255]]);

    let rows = render::ascii(&image, AsciiStyle::Bright, &Charset::default());

    assert!(
        rows[0].starts_with("\x1b[38;2;192;96;0m-\x1b[0m\x1b[38;2;219;109;0m*\x1b[0m"),
        "{rows:?}"
    );
}

#[test]
fn bright_takes_the_2nd_and_98th_percentile() {
    // 100 greys, 0 to 99: lo is the grey at index 2, hi the one at index 98.
    let greys: Vec<[u8; 3]> = (0..100).map(|grey| [grey; 3]).collect();

    let rows = render::ascii(&row(&greys), AsciiStyle::Bright, &Charset::default());

    let cells: Vec<&str> = rows[0].split_inclusive("\x1b[0m").collect();
    assert_eq!(cells.len(), 100);
    for cell in &cells[..3] {
        assert_eq!(*cell, "\x1b[38;2;153;153;153m \x1b[0m");
    }
    assert_ne!(cells[3], cells[2]);
    assert_ne!(cells[97], cells[98]);
    for cell in &cells[98..] {
        assert_eq!(*cell, "\x1b[38;2;255;255;255m@\x1b[0m");
    }
}

#[test]
fn bright_flat_image_uses_plain_luminance() {
    // hi = lo, so the level is Y ÷ 255 000 with no stretch.
    assert_eq!(
        render::ascii(
            &row(&[[200, 10, 10], [200, 10, 10]]),
            AsciiStyle::Bright,
            &Charset::default()
        ),
        ["\x1b[38;2;190;10;10m-\x1b[0m\x1b[38;2;190;10;10m-\x1b[0m"]
    );
    assert_eq!(
        render::ascii(
            &row(&[[128, 128, 128]]),
            AsciiStyle::Bright,
            &Charset::default()
        ),
        ["\x1b[38;2;214;214;214m+\x1b[0m"]
    );
}

#[test]
fn bright_all_transparent_is_spaces() {
    let image = image(2, 1, &[TRANSPARENT, TRANSPARENT]);

    for style in [AsciiStyle::Bright, AsciiStyle::Dense, AsciiStyle::Tinted] {
        assert_eq!(render::ascii(&image, style, &Charset::default()), ["  "]);
    }
}

#[test]
fn bright_skips_transparent_pixels_in_the_percentiles() {
    let image = image(
        3,
        1,
        &[
            TRANSPARENT,
            Rgba([255, 255, 255, 255]),
            Rgba([0, 0, 0, 255]),
        ],
    );

    assert_eq!(
        render::ascii(&image, AsciiStyle::Bright, &Charset::default()),
        [" \x1b[38;2;255;255;255m@\x1b[0m\x1b[38;2;153;153;153m \x1b[0m"]
    );
}

#[test]
fn bright_with_one_character_draws_it_everywhere() {
    let rows = render::ascii(
        &row(&[[0, 0, 0], [255, 255, 255]]),
        AsciiStyle::Bright,
        &Charset::new("#").unwrap(),
    );

    assert_eq!(
        rows,
        ["\x1b[38;2;153;153;153m#\x1b[0m\x1b[38;2;255;255;255m#\x1b[0m"]
    );
}

#[test]
fn dense_has_70_characters_of_its_own() {
    let style = AsciiStyle::Dense;

    assert_eq!(
        render::ascii(&row(&[[0, 0, 0], [255, 255, 255]]), style, &style.charset()),
        ["\x1b[38;2;153;153;153m \x1b[0m\x1b[38;2;255;255;255m$\x1b[0m"]
    );
    for style in [AsciiStyle::Plain, AsciiStyle::Bright, AsciiStyle::Tinted] {
        assert_eq!(style.charset(), Charset::default());
    }
}

#[test]
fn tinted_adds_the_pixel_colour_at_40_percent_behind() {
    assert_eq!(
        render::ascii(
            &row(&[[0, 0, 0], [255, 255, 255]]),
            AsciiStyle::Tinted,
            &Charset::default()
        ),
        [concat!(
            "\x1b[38;2;153;153;153m\x1b[48;2;0;0;0m \x1b[0m",
            "\x1b[38;2;255;255;255m\x1b[48;2;102;102;102m@\x1b[0m"
        )]
    );
}

#[test]
fn ascii_style_goldens() {
    for (style, golden) in [
        (AsciiStyle::Bright, "4x4-ascii-bright.ansi"),
        (AsciiStyle::Dense, "4x4-ascii-dense.ansi"),
        (AsciiStyle::Tinted, "4x4-ascii-tinted.ansi"),
    ] {
        assert_golden(
            &render::ascii(&fixture(), style, &style.charset()),
            golden,
            4,
            4,
        );
    }
}

#[test]
fn braille_opaque_pixels_raise_dots() {
    let red = Rgba([255, 0, 0, 255]);
    let image = image(
        2,
        4,
        &[
            red,
            TRANSPARENT,
            red,
            TRANSPARENT,
            red,
            TRANSPARENT,
            red,
            TRANSPARENT,
        ],
    );

    assert_eq!(render::braille(&image), ["\x1b[38;2;255;0;0m⡇\x1b[0m"]);
}

#[test]
fn braille_transparent_cell_is_space() {
    let image = image(2, 4, &[TRANSPARENT; 8]);

    assert_eq!(render::braille(&image), [" "]);
}

#[test]
fn braille_golden() {
    assert_golden(&render::braille(&fixture()), "4x4-braille.ansi", 2, 1);
}

#[test]
fn braille_missing_pixels_are_transparent() {
    let image = image(1, 1, &[Rgba([255, 0, 0, 255])]);

    assert_eq!(render::braille(&image), ["\x1b[38;2;255;0;0m⠁\x1b[0m"]);
}

#[test]
fn quadrant_transparent_cell_is_space() {
    assert_eq!(render::quadrant(&image(2, 2, &[TRANSPARENT; 4])), [" "]);
}

#[test]
fn quadrant_partial_cell_draws_its_opaque_quarters() {
    let red = Rgba([255, 0, 0, 255]);
    let blue = Rgba([0, 0, 255, 255]);

    assert_eq!(
        render::quadrant(&image(2, 2, &[red, TRANSPARENT, TRANSPARENT, TRANSPARENT])),
        ["\x1b[38;2;255;0;0m▘\x1b[0m"]
    );
    assert_eq!(
        render::quadrant(&image(2, 2, &[TRANSPARENT, blue, TRANSPARENT, blue])),
        ["\x1b[38;2;0;0;255m▐\x1b[0m"]
    );
}

#[test]
fn quadrant_partial_cell_mean_rounds_half_up() {
    let image = image(
        2,
        2,
        &[
            Rgba([0, 0, 0, 255]),
            Rgba([1, 1, 1, 255]),
            TRANSPARENT,
            TRANSPARENT,
        ],
    );

    assert_eq!(render::quadrant(&image), ["\x1b[38;2;1;1;1m▀\x1b[0m"]);
}

#[test]
fn quadrant_full_cell_splits_into_two_colours() {
    let red = Rgba([255, 0, 0, 255]);
    let blue = Rgba([0, 0, 255, 255]);

    // Left column red, right column blue: the foreground is quarters 1 + 4.
    assert_eq!(
        render::quadrant(&image(2, 2, &[red, blue, red, blue])),
        ["\x1b[38;2;255;0;0m\x1b[48;2;0;0;255m▌\x1b[0m"]
    );
}

#[test]
fn quadrant_uniform_cell_takes_the_lowest_split() {
    let grey = Rgba([10, 20, 30, 255]);

    assert_eq!(
        render::quadrant(&image(2, 2, &[grey; 4])),
        ["\x1b[38;2;10;20;30m\x1b[48;2;10;20;30m▘\x1b[0m"]
    );
}

#[test]
fn quadrant_missing_pixels_are_transparent() {
    let red = Rgba([255, 0, 0, 255]);

    assert_eq!(
        render::quadrant(&image(3, 1, &[red; 3])),
        ["\x1b[38;2;255;0;0m▀\x1b[0m\x1b[38;2;255;0;0m▘\x1b[0m"]
    );
}

#[test]
fn quadrant_golden() {
    assert_golden(&render::quadrant(&fixture()), "4x4-quadrant.ansi", 2, 2);
}
