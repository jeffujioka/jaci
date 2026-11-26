//! `jaci encode`, run as the built binary with piped stdio and its config
//! folder and home folder in temporary folders.

mod common;

use std::fs;
use std::path::{Path, PathBuf};

use common::{Env, stderr};
use jaci::art_file::{self, Art};
use tempfile::TempDir;

const IMAGE: &str = "test_fixtures/4x4.png";
const HELP_LINE: &str = "Run 'jaci --help' or 'jaci init --help' for more details.";
const ASCII_GOLDEN: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/test_fixtures/4x4-ascii.ansi");

impl Env {
    fn default_art(&self) -> PathBuf {
        self.config_folder().join("art.ansi")
    }
}

/// Reads the art file at `path`, failing the test when it is not one.
fn art(path: &Path) -> Art {
    art_file::read(path).unwrap_or_else(|error| panic!("{error}"))
}

#[test]
fn broken_config_stops_encode() {
    let env = Env::with_config("[encode");

    let output = env.jaci(&["encode", IMAGE]);

    assert_eq!(output.status.code(), Some(1), "{}", stderr(&output));
    let prefix = format!("jaci: config {}: line 1: ", env.config_path().display());
    assert!(stderr(&output).starts_with(&prefix), "{}", stderr(&output));
    assert!(!env.default_art().exists());
}

#[test]
fn config_value_applies() {
    let env = Env::with_config("[encode]\nwidth = 10\n");

    let output = env.jaci(&["encode", IMAGE]);

    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    assert_eq!(art(&env.default_art()).columns(), 10);
}

#[test]
fn option_beats_config() {
    let env = Env::with_config("[encode]\nwidth = 10\n");

    let output = env.jaci(&["encode", "-w", "20", IMAGE]);

    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    assert_eq!(art(&env.default_art()).columns(), 20);
}

#[test]
fn encode_writes_default_art_file() {
    let env = Env::new();

    let output = env.jaci(&["encode", IMAGE]);

    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    let art = art(&env.default_art());
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
    art(&image);
}

#[test]
fn output_option_names_file() {
    let env = Env::new();
    fs::create_dir_all(env.config_folder()).unwrap();
    let tmp = TempDir::new().unwrap();
    let out = tmp.path().join("out.ansi");

    let output = env.jaci(&["encode", "-o", out.to_str().unwrap(), IMAGE]);

    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    art(&out);
    assert!(!env.default_art().exists());
}

#[test]
fn existing_art_file_replaced() {
    let env = Env::new();
    let tmp = TempDir::new().unwrap();
    let out = tmp.path().join("out.ansi");
    fs::write(&out, "old").unwrap();

    let output = env.jaci(&["encode", "-o", out.to_str().unwrap(), IMAGE]);

    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    let art = art(&out);
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
    assert!(!env.default_art().exists());
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
    assert!(!env.default_art().exists());
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
    assert!(!env.default_art().exists());
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
fn options_pick_mode_and_ratio() {
    let env = Env::new();

    let output = env.jaci(&["encode", "-m", "ascii", "--ratio", "1", "-w", "4", IMAGE]);

    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    assert_eq!(read(&env.default_art()), read(Path::new(ASCII_GOLDEN)));
}

#[test]
fn config_picks_mode_and_ratio() {
    let env = Env::with_config("[encode]\nmode = 'ascii'\nratio = 1\nwidth = 4\n");

    let output = env.jaci(&["encode", IMAGE]);

    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    assert_eq!(read(&env.default_art()), read(Path::new(ASCII_GOLDEN)));
}

#[test]
fn charset_from_config_then_option() {
    let env = Env::with_config("[encode]\nmode = 'ascii'\ncharset = '#'\n");

    for (args, glyph) in [(&[][..], '#'), (&["--charset", "@"][..], '@')] {
        let output = env.jaci(&[&["encode"], args, &[IMAGE]].concat());

        assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
        let glyphs = visible(&read(&env.default_art()));
        assert!(glyphs.contains(glyph), "{glyphs:?}");
        assert!(
            glyphs.chars().all(|c| [glyph, ' ', '\n'].contains(&c)),
            "{glyphs:?}"
        );
    }
}

fn read(path: &Path) -> String {
    fs::read_to_string(path).unwrap()
}

/// `text` without its escape sequences, each `ESC [` up to an `m`.
fn visible(text: &str) -> String {
    let mut visible = String::new();
    let mut chars = text.chars();
    while let Some(c) = chars.next() {
        if c == '\x1b' {
            chars.by_ref().find(|&c| c == 'm');
        } else {
            visible.push(c);
        }
    }
    visible
}
