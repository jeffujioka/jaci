//! `jaci banner`, run as the built binary on a pseudo-terminal of a fixed
//! size, or with piped stdout, its config folder and home folder in
//! temporary folders.

mod common;

use std::fs;
use std::path::Path;
use std::time::Duration;

use common::{
    Env, HELP_LINE, TerminalRun, assert_quiet_success, config_with, in_terminal, read_art,
    read_cache, stderr,
};
use jaci::sysinfo::StaticVariable::Hostname;
use tempfile::TempDir;

const HALFBLOCK: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/test_fixtures/4x4-halfblock.ansi"
);
const ASCII: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/test_fixtures/4x4-ascii.ansi");
const HOSTNAME_CONFIG: &str = "[[banner.text.blocks]]\ncontent = '{hostname}'\n";
const TESTHOST_CACHE: &str = "hostname = \"testhost\"\n";
/// The banner's bound with no commands configured (D41).
const FAST: Duration = Duration::from_millis(100);

impl Env {
    /// Copies the halfblock fixture to the default art file.
    fn add_art(&self) {
        fs::create_dir_all(self.config_folder()).unwrap();
        fs::copy(HALFBLOCK, self.art_path()).unwrap();
    }

    fn add_cache(&self, toml: &str) {
        fs::create_dir_all(self.config_folder()).unwrap();
        fs::write(self.cache_path(), toml).unwrap();
    }

    /// Runs `jaci <args>` on the pseudo-terminal.
    fn banner(&self, args: &[&str]) -> TerminalRun {
        in_terminal(self.command(args))
    }
}

/// A config folder holding `toml`, the halfblock art and a cache holding
/// only `hostname = "testhost"`.
fn set_up(toml: &str) -> Env {
    let env = Env::with_config(toml);
    env.add_art();
    env.add_cache(TESTHOST_CACHE);
    env
}

/// The terminal output of the art at `path` with the default padding and
/// `text` beside its first row.
fn beside(path: &Path, text: &str) -> String {
    let art = read_art(path);
    let rows = art.rows();
    let mut expected = format!("{}    {text}\n", rows[0]);
    for row in &rows[1..] {
        expected.push_str(row);
        expected.push('\n');
    }
    expected
}

fn assert_shown(run: &TerminalRun, expected: &str) {
    assert_eq!(run.status.code(), Some(0), "{}", run.stderr);
    assert_eq!(run.stderr, "");
    assert_eq!(run.stdout, expected);
}

/// Asserts that the banner stopped with the friendly `message`, then the
/// help line, and showed nothing.
fn assert_setup_message(run: &TerminalRun, message: &str) {
    assert_eq!(run.status.code(), Some(1), "{}", run.stderr);
    assert_eq!(run.stderr, format!("{message}\n{HELP_LINE}\n"));
    assert_eq!(run.stdout, "");
}

#[test]
fn banner_shows_art_beside_text() {
    let env = set_up(HOSTNAME_CONFIG);

    let run = env.banner(&["banner"]);

    let art = read_art(Path::new(HALFBLOCK));
    let rows = art.rows();
    assert_eq!(rows.len(), 2);
    assert_shown(&run, &format!("{}    testhost\n{}\n", rows[0], rows[1]));
}

#[test]
fn art_argument_overrides_config() {
    let toml = format!(
        "[banner]\nimage = '{HALFBLOCK}'\n\n{}",
        config_with(&["{hostname}"])
    );
    let env = Env::with_config(&toml);
    env.add_cache(TESTHOST_CACHE);
    let tmp = TempDir::new().unwrap();
    let other = tmp.path().join("other.ansi");
    fs::copy(ASCII, &other).unwrap();

    let run = env.banner(&["banner", other.to_str().unwrap()]);

    assert_shown(&run, &beside(&other, "testhost"));
}

#[test]
fn refresh_cache_writes_cache_first() {
    let env = Env::with_config(HOSTNAME_CONFIG);
    env.add_art();

    let run = env.banner(&["banner", "--refresh-cache"]);

    let cache = read_cache(&env.cache_path());
    let hostname = cache.get(Hostname).expect("the cache has no hostname");
    assert!(!hostname.is_empty());
    assert_shown(&run, &beside(Path::new(HALFBLOCK), hostname));
}

#[test]
fn banner_starts_fast() {
    let env = set_up(&config_with(&["{ram} {disk} {uptime}"]));
    // The first start of a freshly built binary from a process pays for
    // macOS's one-time assessment of it, about 150 ms that is not jaci's.
    let warm_up = env.jaci(&["--version"]);
    assert!(warm_up.status.success(), "{}", stderr(&warm_up));

    let run = env.banner(&["banner"]);

    assert_eq!(run.status.code(), Some(0), "{}", run.stderr);
    assert!(
        run.elapsed < FAST,
        "the banner took {:?}, over the {FAST:?} bound",
        run.elapsed
    );
}

#[test]
fn nothing_set_up() {
    let env = Env::new();

    let run = env.banner(&["banner"]);

    assert_setup_message(
        &run,
        "First time? Welcome! jaci needs a quick setup — run 'jaci init' and you're good to go.",
    );
}

#[test]
fn art_file_missing() {
    let env = Env::new();
    env.add_cache(TESTHOST_CACHE);

    let run = env.banner(&["banner"]);

    assert_setup_message(
        &run,
        "Your banner needs an image! Run 'jaci encode <image>' to turn one into terminal art.",
    );
}

#[test]
fn cache_missing() {
    let env = Env::new();
    env.add_art();

    let run = env.banner(&["banner"]);

    assert_setup_message(&run, "Almost there! Run 'jaci cache' to finish setting up.");
}

#[test]
fn used_static_variable_missing_from_cache() {
    let env = set_up(&config_with(&["{os}"]));

    let run = env.banner(&["banner"]);

    assert_setup_message(&run, "Almost there! Run 'jaci cache' to finish setting up.");
}

#[test]
fn piped_banner_prints_nothing() {
    let env = Env::new();

    let output = env.jaci(&["banner", "--refresh-cache"]);

    assert_quiet_success(&output);
    assert!(!env.cache_path().exists());
}

#[test]
fn dynamic_variables_need_no_external_program() {
    let env = set_up(&config_with(&["{ram}|{disk}|{uptime}"]));
    let mut command = env.command(&["banner"]);
    command.env("PATH", "");

    let run = in_terminal(command);

    assert_eq!(run.status.code(), Some(0), "{}", run.stderr);
    let first_row = run.stdout.lines().next().expect("the banner shows no rows");
    let art = read_art(Path::new(HALFBLOCK));
    let art_row = &art.rows()[0];
    let panel = first_row
        .strip_prefix(&format!("{art_row}    "))
        .unwrap_or_else(|| panic!("row 1 is not the art and the panel: {first_row:?}"));
    let values: Vec<&str> = panel.split('|').collect();
    assert_eq!(values.len(), 3, "{panel:?}");
    for value in values {
        assert!(!value.is_empty() && value != "unknown", "{panel:?}");
    }
}
