//! `jaci init`, run as the built binary with piped stdio and its config
//! folder and home folder in temporary folders.

mod common;

use std::fs;

use common::{Env, HELP_LINE, content_block, read_art, read_cache, stderr, stdout};
use jaci::config::{Banner, Block, Config, Encode, Text};
use jaci::render::Mode;
use jaci::sysinfo::StaticVariable::{Cpu, Gpu, Hostname, Os};

const IMAGE: &str = "test_fixtures/4x4.png";
/// The full config's text blocks, one per built-in variable (D38).
const FULL_BLOCKS: [&str; 7] = [
    "Host: {hostname}",
    "OS: {os}",
    "CPU: {cpu}",
    "GPU: {gpu}",
    "RAM: {ram}",
    "Disk: {disk}",
    "Uptime: {uptime}",
];
/// The keys the full config writes, each with a comment line above it.
const COMMENTED_KEYS: [&str; 10] = [
    "mode", "width", "ratio", "charset", "image", "padding", "align", "valign", "content", "style",
];
const NO_IMAGE: &str = "Which image should jaci use? Run 'jaci init <image>' to get started.";

impl Env {
    fn config_text(&self) -> String {
        fs::read_to_string(self.config_path()).unwrap()
    }
}

/// Text blocks of `contents`, each in the default style and colour.
fn blocks(contents: &[&str]) -> Vec<Block> {
    contents.iter().copied().map(content_block).collect()
}

/// Whether a line of `text` sets `key` right below a comment line.
fn commented(text: &str, key: &str) -> bool {
    let setting = format!("{key} = ");
    let lines: Vec<&str> = text.lines().collect();
    lines
        .windows(2)
        .any(|pair| pair[0].starts_with('#') && pair[1].starts_with(&setting))
}

/// Asserts that the written config is the full config with `mode`, `width`
/// and the text blocks of `contents`, every other key its default.
fn assert_full_config(env: &Env, mode: Mode, width: u32, contents: &[&str]) {
    let expected = Config {
        encode: Encode {
            mode,
            width,
            ..Encode::default()
        },
        banner: Banner {
            image: env.art_path(),
            padding: 4,
            text: Text {
                blocks: blocks(contents),
                ..Text::default()
            },
        },
    };
    assert_eq!(env.config(), expected);
    let text = env.config_text();
    for key in COMMENTED_KEYS {
        assert!(commented(&text, key), "no comment above {key}:\n{text}");
    }
    let comments: Vec<&str> = text.lines().filter(|line| line.starts_with('#')).collect();
    assert!(
        comments
            .iter()
            .any(|line| line.contains("content = ") && line.contains("{cmd:")),
        "no {{cmd:...}} example:\n{text}"
    );
    assert!(
        comments.iter().any(|line| line.contains("command = ")),
        "no command example:\n{text}"
    );
}

#[test]
fn init_sets_everything_up() {
    let env = Env::new();

    let output = env.jaci(&["init", IMAGE]);

    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    assert!(output.stdout.is_empty());
    assert!(output.stderr.is_empty());
    assert_full_config(&env, Mode::Halfblock, 80, &FULL_BLOCKS);
    assert_eq!(read_art(&env.art_path()).columns(), 80);
    let cache = read_cache(&env.cache_path());
    for variable in [Hostname, Os, Cpu, Gpu] {
        assert!(cache.get(variable).is_some(), "{variable:?} is not cached");
    }
}

#[test]
fn minimal_init_writes_lean_config() {
    let env = Env::new();

    let output = env.jaci(&["init", "--minimal", IMAGE]);

    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    let text = env.config_text();
    let lines: Vec<&str> = text.lines().filter(|line| !line.is_empty()).collect();
    assert_eq!(lines.len(), 4, "{text}");
    assert_eq!(lines[0], "[banner]");
    assert!(lines[1].starts_with("image = "), "{text}");
    assert_eq!(
        lines[2..],
        ["[[banner.text.blocks]]", r#"content = "Host: {hostname}""#]
    );
    let expected = Config {
        encode: Encode::default(),
        banner: Banner {
            image: env.art_path(),
            padding: 4,
            text: Text {
                blocks: blocks(&["Host: {hostname}"]),
                ..Text::default()
            },
        },
    };
    assert_eq!(env.config(), expected);
    assert_eq!(read_art(&env.art_path()).columns(), 80);
    let cache = read_cache(&env.cache_path());
    assert!(cache.get(Hostname).is_some());
    for variable in [Os, Cpu, Gpu] {
        assert_eq!(cache.get(variable), None, "{variable:?} is cached");
    }
}

#[test]
fn init_without_image_asks_for_one() {
    let env = Env::new();

    let output = env.jaci(&["init"]);

    assert_eq!(output.status.code(), Some(1));
    assert_eq!(stderr(&output), format!("{NO_IMAGE}\n{HELP_LINE}\n"));
    assert!(!env.config_folder().exists());
}

#[test]
fn declined_overwrite_keeps_config() {
    let env = Env::with_config("# mine\n");

    let output = env.jaci_with_input(&["init", IMAGE], "n\n");

    let path = env.config_path().display().to_string();
    assert_eq!(output.status.code(), Some(1), "{}", stderr(&output));
    assert_eq!(
        stdout(&output),
        format!("{path} already exists. Overwrite it? [y/N] ")
    );
    assert_eq!(
        stderr(&output),
        format!("jaci: init cancelled; {path} left unchanged\n{HELP_LINE}\n")
    );
    assert_eq!(env.config_text(), "# mine\n");
    assert!(!env.art_path().exists());
    assert!(!env.cache_path().exists());
}

#[test]
fn accepted_overwrite_replaces_config() {
    let env = Env::with_config("# mine\n");

    let output = env.jaci_with_input(&["init", IMAGE], "y\n");

    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    assert_full_config(&env, Mode::Halfblock, 80, &FULL_BLOCKS);
}

#[test]
fn interactive_init_asks_for_image() {
    let env = Env::new();

    let output = env.jaci_with_input(&["init", "-i"], &format!("{IMAGE}\n{}", "\n".repeat(9)));

    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    assert_eq!(
        stdout(&output),
        concat!(
            "Image path: ",
            "Render mode (halfblock, ascii, braille) [halfblock]: ",
            "Width in columns [80]: ",
            "Show {hostname}? [Y/n]: ",
            "Show {os}? [Y/n]: ",
            "Show {cpu}? [Y/n]: ",
            "Show {gpu}? [Y/n]: ",
            "Show {ram}? [Y/n]: ",
            "Show {disk}? [Y/n]: ",
            "Show {uptime}? [Y/n]: ",
        )
    );
    assert_full_config(&env, Mode::Halfblock, 80, &FULL_BLOCKS);
    assert_eq!(read_art(&env.art_path()).columns(), 80);
}

#[test]
fn image_answer_expands_home() {
    let env = Env::new();
    fs::copy(IMAGE, env.home().join("logo.png")).unwrap();

    let output = env.jaci_with_input(&["init", "-i"], &format!("~/logo.png\n{}", "\n".repeat(9)));

    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    assert_eq!(read_art(&env.art_path()).columns(), 80);
}

#[test]
fn given_image_skips_image_prompt() {
    let env = Env::new();

    let output = env.jaci_with_input(
        &["init", "-i", IMAGE],
        &format!("braille\n40\n\nn\n{}", "\n".repeat(5)),
    );

    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    let stdout = stdout(&output);
    assert!(!stdout.contains("Image path: "), "{stdout}");
    let text = env.config_text();
    assert!(
        text.lines().any(|line| line == r#"mode = "braille""#),
        "{text}"
    );
    assert!(text.lines().any(|line| line == "width = 40"), "{text}");
    assert!(!text.contains("{os}"), "{text}");
    let mut contents = FULL_BLOCKS.to_vec();
    contents.remove(1);
    assert_full_config(&env, Mode::Braille, 40, &contents);
    assert_eq!(read_art(&env.art_path()).columns(), 40);
}

#[test]
fn invalid_answer_stops_init() {
    let env = Env::new();

    let output = env.jaci_with_input(&["init", "-i", IMAGE], "sepia\n");

    assert_eq!(output.status.code(), Some(1));
    assert_eq!(
        stderr(&output),
        format!("jaci: 'sepia' is not a valid render mode\n{HELP_LINE}\n")
    );
    assert!(!env.config_folder().exists());
}
