//! `jaci init`, run as the built binary with piped stdio and its config
//! folder and home folder in temporary folders.

mod common;

use std::fs;
use std::os::unix::fs::symlink;

use common::{Env, HELP_LINE, content_block, read_art, read_cache, read_png, stderr, stdout};
use jaci::cache;
use jaci::config::{Banner, Block, Config, Encode, Text, Valign};
use jaci::init::{self, Settings};
use jaci::named::Named;
use jaci::render::{AsciiStyle, Mode};
use jaci::sysinfo::StaticVariable::{self, Cpu, Gpu, Hostname, Os, User};

const IMAGE: &str = "test_fixtures/4x4.png";
/// The keys the full config writes, each with a comment line above it.
const COMMENTED_KEYS: [&str; 16] = [
    "mode",
    "ascii_style",
    "width",
    "min_width",
    "max_width",
    "ratio",
    "image",
    "padding",
    "align",
    "valign",
    "icons",
    "kind",
    "label",
    "icon",
    "content",
    "style",
];
const NO_IMAGE: &str = "Which image should jaci use? Run 'jaci init <image>' to get started.";

impl Env {
    fn config_text(&self) -> String {
        fs::read_to_string(self.config_path()).unwrap()
    }
}

/// The settings of `jaci init` without `-i` but for `mode`, `style` and
/// `width`.
fn settings(mode: Mode, style: AsciiStyle, width: u32) -> Settings {
    Settings {
        mode,
        ascii_style: style,
        width,
        ..Settings::default()
    }
}

/// Whether a line of `text` sets `key` right below a comment line.
fn commented(text: &str, key: &str) -> bool {
    let setting = format!("{key} = ");
    let lines: Vec<&str> = text.lines().collect();
    lines
        .windows(2)
        .any(|pair| pair[0].starts_with('#') && pair[1].starts_with(&setting))
}

/// Asserts that the written config is the full config of `settings`, its
/// blocks made with `{cpu}` and `{gpu}` as this test resolves them, every
/// other key its default.
fn assert_full_config(env: &Env, settings: &Settings) {
    let (cpu, gpu) = (Cpu.resolve(), Gpu.resolve());
    let expected = Config {
        encode: settings.encode(),
        banner: Banner {
            image: env.art_path(),
            padding: 4,
            text: Text {
                valign: Valign::Center,
                icons: true,
                blocks: init::blocks(settings, || (&cpu, &gpu)),
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
        comments.iter().any(|line| line.starts_with("# charset = ")),
        "no commented-out charset:\n{text}"
    );
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
    assert_full_config(&env, &Settings::default());
    assert_eq!(read_art(&env.art_path()).columns(), 80);
    let config = env.config();
    assert_eq!(config.banner.text.valign, Valign::Center);
    assert!(config.banner.text.icons);
    let blocks = &config.banner.text.blocks;
    assert_eq!(blocks.first(), Some(&Block::Title));
    assert_eq!(blocks.last(), Some(&Block::Dots));
    let text = env.config_text();
    let lines: Vec<&str> = text.lines().collect();
    assert!(lines.contains(&r#"icon = "\uF108""#), "{text}");
    assert!(
        lines.windows(2).any(|pair| pair[0].starts_with('#')
            && pair[0].contains("Nerd Font")
            && pair[1] == "icons = true"),
        "no Nerd Font comment above icons:\n{text}"
    );
    let cache = read_cache(&env.cache_path());
    let used = cache::used_variables(&config);
    assert!(used.contains(&User) && used.contains(&Hostname), "{used:?}");
    for &variable in StaticVariable::ALL {
        assert_eq!(
            cache.get(variable).is_some(),
            used.contains(&variable),
            "{variable:?} cached"
        );
    }
}

#[test]
fn init_writes_size_limits() {
    let env = Env::new();

    let output = env.jaci(&["init", IMAGE]);

    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    let text = env.config_text();
    let encode: Vec<&str> = text
        .lines()
        .skip_while(|line| *line != "[encode]")
        .skip(1)
        .take_while(|line| !line.starts_with('['))
        .collect();
    let position = |line: &str| encode.iter().position(|candidate| *candidate == line);
    let width = position("width = 80").expect(&text);
    let min = position("min_width = 20").expect(&text);
    let max = position("max_width = 60").expect(&text);
    assert!(width < min && min < max, "{text}");
    for limit in [min, max] {
        assert!(
            encode[limit - 1].starts_with('#'),
            "no comment above:\n{text}"
        );
    }
    let config = env.config();
    assert_eq!((config.encode.min_width, config.encode.max_width), (20, 60));
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
                blocks: vec![content_block("Host: {hostname}")],
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
fn init_keeps_saved_image() {
    let env = Env::new();

    let output = env.jaci(&["init", "--minimal", IMAGE]);

    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    let saved = read_png(&env.config_folder().join("image.png"));
    assert_eq!(saved, image::open(IMAGE).unwrap().into_rgba8());
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
    assert_full_config(&env, &Settings::default());
}

#[test]
fn accepted_overwrite_writes_through_a_symlinked_config() {
    let env = Env::new();
    let dotfiles = env.home().join("dotfiles");
    let target = dotfiles.join("config.toml");
    fs::create_dir_all(&dotfiles).unwrap();
    fs::write(&target, "# mine\n").unwrap();
    fs::create_dir_all(env.config_folder()).unwrap();
    symlink(&target, env.config_path()).unwrap();

    let output = env.jaci_with_input(&["init", IMAGE], "y\n");

    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    let link = fs::symlink_metadata(env.config_path()).unwrap();
    assert!(link.file_type().is_symlink());
    assert_eq!(fs::read_link(env.config_path()).unwrap(), target);
    assert_full_config(&env, &Settings::default());
}

#[test]
fn dangling_symlinked_config_asks_to_overwrite() {
    let env = Env::new();
    fs::create_dir_all(env.config_folder()).unwrap();
    symlink(env.home().join("dotfiles/config.toml"), env.config_path()).unwrap();

    let output = env.jaci_with_input(&["init", IMAGE], "n\n");

    let path = env.config_path().display().to_string();
    assert_eq!(output.status.code(), Some(1), "{}", stderr(&output));
    assert_eq!(
        stdout(&output),
        format!("{path} already exists. Overwrite it? [y/N] ")
    );
    assert!(!env.art_path().exists());
}

#[test]
fn interactive_init_asks_for_image() {
    let env = Env::new();

    let output = env.jaci_with_input(&["init", "-i"], &format!("{IMAGE}\n{}", "\n".repeat(10)));

    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    assert_eq!(
        stdout(&output),
        concat!(
            "Image path: ",
            "Render mode (ascii, halfblock, quadrant, braille) [ascii]: ",
            "Ascii style (plain, bright, dense, tinted) [bright]: ",
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
    assert_full_config(&env, &Settings::default());
    assert_eq!(read_art(&env.art_path()).columns(), 80);
}

#[test]
fn image_answer_expands_home() {
    let env = Env::new();
    fs::copy(IMAGE, env.home().join("logo.png")).unwrap();

    let output = env.jaci_with_input(&["init", "-i"], &format!("~/logo.png\n{}", "\n".repeat(10)));

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
    assert!(!stdout.contains("Ascii style"), "{stdout}");
    let text = env.config_text();
    assert!(
        text.lines().any(|line| line == r#"mode = "braille""#),
        "{text}"
    );
    assert!(text.lines().any(|line| line == "width = 40"), "{text}");
    assert!(!text.contains("{os}"), "{text}");
    let mut settings = settings(Mode::Braille, AsciiStyle::Bright, 40);
    settings.shown[1] = false;
    assert_full_config(&env, &settings);
    assert_eq!(read_art(&env.art_path()).columns(), 40);
}

#[test]
fn interactive_init_asks_the_style_for_ascii() {
    let env = Env::new();

    let output = env.jaci_with_input(
        &["init", "-i", IMAGE],
        &format!("ascii\ntinted\n40\n{}", "\n".repeat(7)),
    );

    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    assert!(
        stdout(&output).starts_with(concat!(
            "Render mode (ascii, halfblock, quadrant, braille) [ascii]: ",
            "Ascii style (plain, bright, dense, tinted) [bright]: ",
            "Width in columns [80]: ",
        )),
        "{}",
        stdout(&output)
    );
    let text = env.config_text();
    assert!(
        text.lines().any(|line| line == r#"ascii_style = "tinted""#),
        "{text}"
    );
    assert_full_config(&env, &settings(Mode::Ascii, AsciiStyle::Tinted, 40));
    assert_eq!(read_art(&env.art_path()).columns(), 40);
}

#[test]
fn invalid_style_stops_init() {
    let env = Env::new();

    let output = env.jaci_with_input(&["init", "-i", IMAGE], "ascii\nneon\n");

    assert_eq!(output.status.code(), Some(1));
    assert_eq!(
        stderr(&output),
        format!("jaci: 'neon' is not a valid ascii style\n{HELP_LINE}\n")
    );
    assert!(!env.config_folder().exists());
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

#[test]
fn missing_image_stops_init_before_the_questions() {
    let env = Env::new();

    let output = env.jaci_with_input(&["init", "-i", "missing.png"], &"\n".repeat(10));

    assert_eq!(output.status.code(), Some(1));
    assert_eq!(
        stderr(&output),
        format!("jaci: image not found: missing.png\n{HELP_LINE}\n")
    );
    let stdout = stdout(&output);
    assert!(!stdout.contains("Render mode"), "{stdout}");
    assert!(!env.config_folder().exists());
}

#[test]
fn too_large_width_writes_nothing() {
    let env = Env::new();

    let output = env.jaci_with_input(
        &["init", "-i", IMAGE],
        &format!("halfblock\n20000\n{}", "\n".repeat(7)),
    );

    assert_eq!(output.status.code(), Some(1), "{}", stderr(&output));
    assert_eq!(
        stderr(&output),
        format!("jaci: art of 20000×10000 cells is too large\n{HELP_LINE}\n")
    );
    assert!(!env.config_folder().exists());
}

#[test]
fn help_tells_of_the_saved_image() {
    let output = Env::new().jaci(&["init", "--help"]);

    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    let help = stdout(&output);
    assert!(help.contains("keep the image as the saved image"), "{help}");
}
