//! `jaci encode`, run as the built binary with piped stdio and its config
//! folder and home folder in temporary folders.

mod common;

use std::fs;
use std::path::Path;

use common::{Env, HELP_LINE, plain, read_art, read_png, stderr};
use tempfile::TempDir;

const IMAGE: &str = "test_fixtures/4x4.png";
const ASCII_GOLDEN: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/test_fixtures/4x4-ascii.ansi");

#[test]
fn broken_config_stops_encode() {
    let env = Env::with_config("[encode");

    let output = env.jaci(&["encode", IMAGE]);

    assert_eq!(output.status.code(), Some(1), "{}", stderr(&output));
    let prefix = format!("jaci: config {}: line 1: ", env.config_path().display());
    assert!(stderr(&output).starts_with(&prefix), "{}", stderr(&output));
    assert!(!env.art_path().exists());
}

#[test]
fn config_value_applies() {
    let env = Env::with_config("[encode]\nwidth = 10\n");

    let output = env.jaci(&["encode", IMAGE]);

    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    assert_eq!(read_art(&env.art_path()).columns(), 10);
}

#[test]
fn option_beats_config() {
    let env = Env::with_config("[encode]\nwidth = 10\n");

    let output = env.jaci(&["encode", "-w", "20", IMAGE]);

    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    assert_eq!(read_art(&env.art_path()).columns(), 20);
}

#[test]
fn encode_writes_default_art_file() {
    let env = Env::new();

    let output = env.jaci(&["encode", IMAGE]);

    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    let art = read_art(&env.art_path());
    assert_eq!((art.columns(), art.rows().len()), (80, 40));
    assert!(output.stdout.is_empty());
    assert!(output.stderr.is_empty());
}

#[test]
fn encode_writes_config_image_path() {
    let tmp = TempDir::new().unwrap();
    let image = tmp.path().join("a/b/art.ansi");
    let env = Env::with_config(&format!("[banner]\nimage = '{}'\n", image.display()));

    let output = env.jaci(&["encode", IMAGE]);

    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    read_art(&image);
}

#[test]
fn output_option_names_file() {
    let env = Env::new();
    fs::create_dir_all(env.config_folder()).unwrap();
    let tmp = TempDir::new().unwrap();
    let out = tmp.path().join("out.ansi");

    let output = env.jaci(&["encode", "-o", out.to_str().unwrap(), IMAGE]);

    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    read_art(&out);
    assert!(!env.art_path().exists());
}

#[test]
fn existing_art_file_replaced() {
    let env = Env::new();
    let tmp = TempDir::new().unwrap();
    let out = tmp.path().join("out.ansi");
    fs::write(&out, "old").unwrap();

    let output = env.jaci(&["encode", "-o", out.to_str().unwrap(), IMAGE]);

    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    let art = read_art(&out);
    assert_eq!((art.columns(), art.rows().len()), (80, 40));
}

#[test]
fn error_ends_with_help_line() {
    let env = Env::new();

    let output = env.jaci(&["encode", "missing.png"]);

    assert_eq!(output.status.code(), Some(1));
    assert_eq!(
        stderr(&output),
        format!("jaci: image not found: missing.png\n{HELP_LINE}\n")
    );
}

#[test]
fn width_and_scale_refused() {
    let env = Env::new();
    fs::create_dir_all(env.config_folder()).unwrap();

    let output = env.jaci(&["encode", "-w", "40", "-s", "50", IMAGE]);

    assert_eq!(output.status.code(), Some(2), "{}", stderr(&output));
    assert!(!env.art_path().exists());
}

#[test]
fn out_of_range_options_are_usage_errors() {
    let env = Env::new();

    for args in [
        ["-w", "0"],
        ["-s", "0"],
        ["--ratio", "0"],
        ["--ratio", "-1"],
        ["--ratio", "inf"],
        ["--charset", ""],
        ["--mode", "sixel"],
        ["--widht", "10"],
    ] {
        let output = env.jaci(&["encode", args[0], args[1], IMAGE]);

        assert_eq!(
            output.status.code(),
            Some(2),
            "{args:?}: {}",
            stderr(&output)
        );
    }
    assert!(!env.art_path().exists());
}

#[test]
fn auto_needs_a_terminal() {
    let env = Env::new();

    let output = env.jaci(&["encode", "--auto", IMAGE]);

    assert_eq!(output.status.code(), Some(1));
    assert_eq!(
        stderr(&output),
        format!("jaci: --auto needs a terminal to measure\n{HELP_LINE}\n")
    );
    assert!(!env.art_path().exists());
}

#[test]
fn write_failure_names_art_file() {
    let env = Env::new();
    let tmp = TempDir::new().unwrap();
    let blocker = tmp.path().join("file");
    fs::write(&blocker, "").unwrap();
    let out = blocker.join("out.ansi");

    let output = env.jaci(&["encode", "-o", out.to_str().unwrap(), IMAGE]);

    assert_eq!(output.status.code(), Some(1));
    let prefix = format!("jaci: cannot write art file {}: ", out.display());
    let stderr = stderr(&output);
    assert!(stderr.starts_with(&prefix), "{stderr}");
    assert!(stderr.ends_with(&format!("\n{HELP_LINE}\n")), "{stderr}");
    assert_eq!(stderr.lines().count(), 2, "{stderr}");
}

#[test]
fn art_too_large_is_refused() {
    let env = Env::new();

    for (args, cells) in [
        (
            &["-w", "4294967295", "-m", "braille"][..],
            "4294967295×2147483648",
        ),
        (&["-w", "20000"][..], "20000×10000"),
    ] {
        let output = env.jaci(&[&["encode"], args, &[IMAGE]].concat());

        assert_eq!(
            output.status.code(),
            Some(1),
            "{args:?}: {}",
            stderr(&output)
        );
        assert_eq!(
            stderr(&output),
            format!("jaci: art of {cells} cells is too large\n{HELP_LINE}\n"),
            "{args:?}"
        );
    }
    assert!(!env.art_path().exists());
}

#[test]
fn options_pick_mode_and_ratio() {
    let env = Env::new();

    let output = env.jaci(&[
        "encode",
        "-m",
        "ascii",
        "--ascii-style",
        "plain",
        "--ratio",
        "1",
        "-w",
        "4",
        IMAGE,
    ]);

    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    assert_eq!(read(&env.art_path()), read(Path::new(ASCII_GOLDEN)));
}

#[test]
fn config_picks_mode_and_ratio() {
    let env =
        Env::with_config("[encode]\nmode = 'ascii'\nascii_style = 'plain'\nratio = 1\nwidth = 4\n");

    let output = env.jaci(&["encode", IMAGE]);

    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    assert_eq!(read(&env.art_path()), read(Path::new(ASCII_GOLDEN)));
}

#[test]
fn charset_from_config_then_option() {
    let env = Env::with_config("[encode]\nmode = 'ascii'\ncharset = '#'\n");

    for (args, glyph) in [(&[][..], '#'), (&["--charset", "@"][..], '@')] {
        let output = env.jaci(&[&["encode"], args, &[IMAGE]].concat());

        assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
        let glyphs = plain(&read(&env.art_path()));
        assert!(glyphs.contains(glyph), "{glyphs:?}");
        assert!(
            glyphs.chars().all(|c| [glyph, ' ', '\n'].contains(&c)),
            "{glyphs:?}"
        );
    }
}

fn golden(name: &str) -> String {
    read(
        &Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("test_fixtures")
            .join(name),
    )
}

#[test]
fn default_is_bright_ascii() {
    let env = Env::new();

    // 4 columns at ratio 1 is the fixture's own 4×4 pixels, unresampled.
    let output = env.jaci(&["encode", "--ratio", "1", "-w", "4", IMAGE]);

    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    assert_eq!(read(&env.art_path()), golden("4x4-ascii-bright.ansi"));
}

#[test]
fn ascii_style_from_config_then_option() {
    let env = Env::with_config("[encode]\nascii_style = 'tinted'\nratio = 1\nwidth = 4\n");

    for (args, name) in [
        (&[][..], "4x4-ascii-tinted.ansi"),
        (&["--ascii-style", "dense"][..], "4x4-ascii-dense.ansi"),
    ] {
        let output = env.jaci(&[&["encode"], args, &[IMAGE]].concat());

        assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
        assert_eq!(read(&env.art_path()), golden(name));
    }
}

#[test]
fn charset_beats_the_dense_style() {
    for (config, args) in [
        ("", &["--ascii-style", "dense", "--charset", "#"][..]),
        ("[encode]\ncharset = '#'\n", &["--ascii-style", "dense"][..]),
    ] {
        let env = Env::with_config(config);

        let output = env.jaci(&[&["encode"], args, &[IMAGE]].concat());

        assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
        let glyphs = plain(&read(&env.art_path()));
        assert!(glyphs.contains('#'), "{glyphs:?}");
        assert!(
            glyphs.chars().all(|c| ['#', ' ', '\n'].contains(&c)),
            "{glyphs:?}"
        );
    }
}

#[test]
fn ascii_style_is_ignored_outside_ascii() {
    let env = Env::new();

    // 4 columns at ratio 0.5 is 4×2 halfblock cells: the fixture unresampled.
    let output = env.jaci(&[
        "encode",
        "-m",
        "halfblock",
        "--ascii-style",
        "tinted",
        "-w",
        "4",
        IMAGE,
    ]);

    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    assert_eq!(read(&env.art_path()), golden("4x4-halfblock.ansi"));
}

#[test]
fn quadrant_mode_from_the_option() {
    let env = Env::new();

    // 2 columns at ratio 1 is 2×2 quadrant cells: the fixture unresampled.
    let output = env.jaci(&["encode", "-m", "quadrant", "--ratio", "1", "-w", "2", IMAGE]);

    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    assert_eq!(read(&env.art_path()), golden("4x4-quadrant.ansi"));
}

#[test]
fn unknown_ascii_style_is_a_usage_error() {
    let env = Env::new();

    let output = env.jaci(&["encode", "--ascii-style", "neon", IMAGE]);

    assert_eq!(output.status.code(), Some(2));
    assert!(!env.art_path().exists());
}

#[test]
fn encode_keeps_saved_image() {
    let env = Env::new();

    let output = env.jaci(&["encode", IMAGE]);

    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    let saved = read_png(&env.config_folder().join("image.png"));
    assert_eq!(saved, image::open(IMAGE).unwrap().into_rgba8());
}

#[test]
fn large_image_is_shrunk() {
    let env = Env::new();
    let tmp = TempDir::new().unwrap();
    let large = tmp.path().join("large.png");
    image::RgbaImage::from_pixel(300, 150, image::Rgba([10, 20, 30, 255]))
        .save(&large)
        .unwrap();

    let output = env.jaci(&["encode", large.to_str().unwrap()]);

    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    let saved = read_png(&env.config_folder().join("image.png"));
    assert_eq!(saved.dimensions(), (120, 60));
}

#[test]
fn output_option_keeps_no_saved_image() {
    let env = Env::new();
    let tmp = TempDir::new().unwrap();
    let out = tmp.path().join("out.ansi");

    let output = env.jaci(&["encode", "-o", out.to_str().unwrap(), IMAGE]);

    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    assert!(!env.config_folder().join("image.png").exists());
    assert!(!env.config_folder().join("sizes").exists());
}

#[test]
fn new_saved_image_drops_sized_art() {
    let env = Env::new();
    let sizes = env.config_folder().join("sizes");
    fs::create_dir_all(&sizes).unwrap();
    fs::write(sizes.join("38.ansi"), "old").unwrap();

    let output = env.jaci(&["encode", IMAGE]);

    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    let left = fs::read_dir(&sizes).map_or(0, Iterator::count);
    assert_eq!(left, 0, "sized art left in {}", sizes.display());
}

fn read(path: &Path) -> String {
    fs::read_to_string(path).unwrap()
}
