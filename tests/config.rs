use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};

use jaci::config::{self, Align, Banner, Config, Encode, Text, Valign};
use jaci::render::{Charset, Mode};
use tempfile::TempDir;

const HOME: &str = "/h";

/// Loads `toml` as the `config.toml` of a fresh config folder, giving the
/// folder (kept alive) and the path the messages name.
fn load(toml: &str) -> (TempDir, PathBuf, Result<Config, config::ConfigError>) {
    let folder = TempDir::new().unwrap();
    let path = folder.path().join("config.toml");
    fs::write(&path, toml).unwrap();
    let config = config::load(folder.path(), Path::new(HOME));
    (folder, path, config)
}

fn load_error(toml: &str) -> (PathBuf, String) {
    let (_folder, path, config) = load(toml);
    (path, config.unwrap_err().to_string())
}

#[test]
fn xdg_config_home_wins() {
    let folder = config::folder(Some(OsStr::new("/x")), Path::new(HOME));

    assert_eq!(folder, Path::new("/x/jaci"));
}

#[test]
fn home_config_folder_otherwise() {
    for xdg_config_home in [None, Some(OsStr::new(""))] {
        let folder = config::folder(xdg_config_home, Path::new(HOME));

        assert_eq!(folder, Path::new("/h/.config/jaci"), "{xdg_config_home:?}");
    }
}

#[test]
fn missing_config_gives_defaults() {
    let folder = TempDir::new().unwrap();

    let config = config::load(folder.path(), Path::new(HOME)).unwrap();

    assert_eq!(
        config,
        Config {
            encode: Encode {
                mode: Mode::Halfblock,
                width: 80,
                ratio: 0.5,
                charset: Charset::default(),
            },
            banner: Banner {
                image: folder.path().join("art.ansi"),
                padding: 4,
                text: Text {
                    align: Align::Left,
                    valign: Valign::Top,
                    blocks: Vec::new(),
                },
            },
        }
    );
}

#[test]
fn syntax_error_names_line() {
    let (path, message) = load_error("[encode]\nwidth = \n");

    let prefix = format!("jaci: config {}: line 2: ", path.display());
    assert!(message.starts_with(&prefix), "{message}");
}

#[test]
fn out_of_range_value_names_key() {
    let (path, message) = load_error("[encode]\nwidth = -5\n");

    assert_eq!(
        message,
        format!(
            "jaci: config {}: line 2: encode.width must be a whole number of at least 1",
            path.display()
        )
    );
}

#[test]
fn unknown_key_refused() {
    let (path, message) = load_error("[encode]\nwidht = 10\n");

    assert_eq!(
        message,
        format!(
            "jaci: config {}: line 2: unknown key encode.widht",
            path.display()
        )
    );
}

#[test]
fn block_with_command_and_content_refused() {
    let (path, message) =
        load_error("[[banner.text.blocks]]\ncontent = \"a\"\ncommand = \"hostname\"\n");

    assert_eq!(
        message,
        format!(
            "jaci: config {}: line 1: banner.text.blocks[1] needs exactly one of content and command",
            path.display()
        )
    );
}

#[test]
fn unclosed_inline_command_refused() {
    let (path, message) = load_error("[[banner.text.blocks]]\ncontent = \"IP: {cmd:hostname\"\n");

    assert_eq!(
        message,
        format!(
            "jaci: config {}: line 2: banner.text.blocks[1].content has an unclosed {{cmd:",
            path.display()
        )
    );
}

#[test]
fn image_path_expands_home() {
    let (_folder, _path, config) = load("[banner]\nimage = \"~/art.ansi\"\n");

    assert_eq!(config.unwrap().banner.image, Path::new("/h/art.ansi"));
}
