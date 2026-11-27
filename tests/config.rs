use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};

use jaci::config::{
    self, Align, Banner, Block, BlockText, Config, Encode, Style, Text, TextBlock, Valign,
};
use jaci::render::{AsciiStyle, Charset, Mode};
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
                mode: Mode::Ascii,
                width: 80,
                min_width: 20,
                max_width: 60,
                ratio: 0.5,
                ascii_style: AsciiStyle::Bright,
                charset: None,
            },
            banner: Banner {
                image: folder.path().join("art.ansi"),
                padding: 4,
                text: Text {
                    align: Align::Left,
                    valign: Valign::Top,
                    icons: true,
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
ascii_style = "tinted"
width = 120
min_width = 30
max_width = 100
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
                min_width: 30,
                max_width: 100,
                ratio: 0.45,
                ascii_style: AsciiStyle::Tinted,
                charset: Some(Charset::new(" #").unwrap()),
            },
            banner: Banner {
                image: Path::new(HOME).join("art/logo.ansi"),
                padding: 2,
                text: Text {
                    align: Align::Right,
                    valign: Valign::Center,
                    icons: true,
                    blocks: vec![
                        Block::Text(TextBlock {
                            text: BlockText::Content(vec![
                                Segment::Text("Shell: ".to_owned()),
                                Segment::Command("basename $SHELL".to_owned()),
                            ]),
                            style: Style::Bold,
                            color: Some([0x12, 0xAB, 0x34]),
                            label: None,
                            icon: None,
                        }),
                        Block::Text(TextBlock {
                            text: BlockText::Command("date".to_owned()),
                            style: Style::Dim,
                            color: None,
                            label: None,
                            icon: None,
                        }),
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
fn size_limits_load() {
    let (_folder, _path, config) = load("[encode]\nmin_width = 10\nmax_width = 40\n");

    let encode = config.unwrap_or_else(|error| panic!("{error}")).encode;
    assert_eq!((encode.min_width, encode.max_width), (10, 40));
}

#[test]
fn size_limits_default_to_20_and_60() {
    let (_folder, _path, config) = load("[encode]\nwidth = 50\n");

    let encode = config.unwrap_or_else(|error| panic!("{error}")).encode;
    assert_eq!((encode.min_width, encode.max_width), (20, 60));
}

#[test]
fn minimum_above_maximum_refused() {
    let (path, message) = load_error("[encode]\nmin_width = 30\nmax_width = 25\n");

    let prefix = format!("jaci: config {}: line 2: ", path.display());
    assert!(message.starts_with(&prefix), "{message}");
    assert!(
        message.contains("encode.min_width (30) must be at most encode.max_width (25)"),
        "{message}"
    );
}

#[test]
fn maximum_below_default_minimum_refused() {
    let (path, message) = load_error("[encode]\nmax_width = 10\n");

    let prefix = format!("jaci: config {}: line 2: ", path.display());
    assert!(message.starts_with(&prefix), "{message}");
    assert!(
        message.contains("encode.min_width (20) must be at most encode.max_width (10)"),
        "{message}"
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
fn bad_ascii_style_names_key() {
    let (path, message) = load_error("[encode]\nascii_style = \"neon\"\n");

    assert_eq!(
        message,
        format!(
            "jaci: config {}: line 2: encode.ascii_style must be one of plain, bright, dense, tinted",
            path.display()
        )
    );
}

#[test]
fn image_path_expands_home() {
    let (_folder, _path, config) = load("[banner]\nimage = \"~/art.ansi\"\n");

    assert_eq!(config.unwrap().banner.image, Path::new("/h/art.ansi"));
}

#[test]
fn unknown_kind_refused() {
    let (path, message) = load_error("[[banner.text.blocks]]\nkind = \"card\"\n");

    assert_eq!(
        message,
        format!(
            "jaci: config {}: line 2: banner.text.blocks[1].kind must be one of text, title, dots",
            path.display()
        )
    );
}

#[test]
fn title_block_with_content_refused() {
    let (path, message) =
        load_error("[[banner.text.blocks]]\nkind = \"title\"\ncontent = \"hi\"\n");

    assert_eq!(
        message,
        format!(
            "jaci: config {}: line 3: banner.text.blocks[1].content is not allowed in a title block",
            path.display()
        )
    );
}

#[test]
fn icons_must_be_true_or_false() {
    let (path, message) = load_error("[banner.text]\nicons = \"no\"\n");

    assert_eq!(
        message,
        format!(
            "jaci: config {}: line 2: banner.text.icons must be true or false",
            path.display()
        )
    );
}

#[test]
fn labelled_block_still_needs_text() {
    let (path, message) = load_error("[[banner.text.blocks]]\nlabel = \"os\"\n");

    assert_eq!(
        message,
        format!(
            "jaci: config {}: line 1: banner.text.blocks[1] needs exactly one of content and command",
            path.display()
        )
    );
}

#[test]
fn new_block_keys_load() {
    let (_folder, _path, config) = load(
        r#"
[banner.text]
icons = false

[[banner.text.blocks]]
kind = "title"

[[banner.text.blocks]]
label = "os"
icon = "\uF108"
content = "{os}"
"#,
    );

    let text = config.unwrap_or_else(|error| panic!("{error}")).banner.text;
    assert!(!text.icons);
    assert_eq!(
        text.blocks,
        [
            Block::Title,
            Block::Text(TextBlock {
                text: BlockText::Content(vec![Segment::Text("{os}".to_owned())]),
                style: Style::Normal,
                color: None,
                label: Some("os".to_owned()),
                icon: Some("\u{F108}".to_owned()),
            }),
        ]
    );
}
