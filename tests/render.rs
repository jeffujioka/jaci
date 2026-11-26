use std::fs;
use std::path::Path;

use image::{Rgba, RgbaImage};
use jaci::art_file;
use jaci::render::{self, Charset};
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
        render::ascii(&image, &Charset::default()),
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
        render::ascii(&image, &charset),
        ["\x1b[38;2;100;100;100ma\x1b[0m\x1b[38;2;200;200;200mb\x1b[0m"]
    );
}

#[test]
fn ascii_transparent_pixel_is_space() {
    let image = image(1, 1, &[TRANSPARENT]);

    assert_eq!(render::ascii(&image, &Charset::default()), [" "]);
}

#[test]
fn ascii_exact_luminance_boundary() {
    // L is exactly 128, so the index is ⌊128 × 10 ÷ 256⌋ = 5.
    let image = image(1, 1, &[Rgba([128, 128, 128, 255])]);

    assert_eq!(
        render::ascii(&image, &Charset::default()),
        ["\x1b[38;2;128;128;128m+\x1b[0m"]
    );
}

#[test]
fn ascii_golden() {
    assert_golden(
        &render::ascii(&fixture(), &Charset::default()),
        "4x4-ascii.ansi",
        4,
        4,
    );
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
