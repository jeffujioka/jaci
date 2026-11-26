use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};

use jaci::config::{self, Align, Banner, Block, BlockText, Config, Encode, Style, Text, Valign};
use jaci::render::{Charset, Mode};
use jaci::template::Segment;
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
fn every_key_loads_its_value() {
    let (_folder, _path, config) = load(
        r##"
[encode]
mode = "braille"
width = 120
ratio = 0.45
charset = " #"

[banner]
image = "~/art/logo.ansi"
padding = 2

[banner.text]
align = "right"
valign = "center"

[[banner.text.blocks]]
content = "Shell: {cmd:basename $SHELL}"
style = "bold"
color = "#12aB34"

[[banner.text.blocks]]
command = "date"
style = "dim"
"##,
    );

    assert_eq!(
        config.unwrap_or_else(|error| panic!("{error}")),
        Config {
            encode: Encode {
                mode: Mode::Braille,
                width: 120,
                ratio: 0.45,
                charset: Charset::new(" #").unwrap(),
            },
            banner: Banner {
                image: Path::new(HOME).join("art/logo.ansi"),
                padding: 2,
                text: Text {
                    align: Align::Right,
                    valign: Valign::Center,
                    blocks: vec![
                        Block {
                            text: BlockText::Content(vec![
                                Segment::Text("Shell: ".to_owned()),
                                Segment::Command("basename $SHELL".to_owned()),
                            ]),
                            style: Style::Bold,
                            color: Some([0x12, 0xAB, 0x34]),
                        },
                        Block {
                            text: BlockText::Command("date".to_owned()),
                            style: Style::Dim,
                            color: None,
                        },
                    ],
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
fn empty_inline_command_refused() {
    for content in ["IP: {cmd:} up", "{cmd: \t }"] {
        let (path, message) = load_error(&format!(
            "[[banner.text.blocks]]\ncontent = \"{content}\"\n"
        ));

        assert_eq!(
            message,
            format!(
                "jaci: config {}: line 2: banner.text.blocks[1].content has an empty {{cmd:}}",
                path.display()
            ),
            "{content:?}"
        );
    }
}

#[test]
fn blank_block_command_refused() {
    for command in ["", " \t "] {
        let (path, message) = load_error(&format!(
            "[[banner.text.blocks]]\ncommand = \"{command}\"\n"
        ));

        assert_eq!(
            message,
            format!(
                "jaci: config {}: line 2: banner.text.blocks[1].command must be a shell command",
                path.display()
            ),
            "{command:?}"
        );
    }
}

#[test]
fn image_path_expands_home() {
    let (_folder, _path, config) = load("[banner]\nimage = \"~/art.ansi\"\n");

    assert_eq!(config.unwrap().banner.image, Path::new("/h/art.ansi"));
}
