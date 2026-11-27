//! `jaci banner`, run as the built binary on a pseudo-terminal of a fixed
//! size, or with piped stdout, its config folder and home folder in
//! temporary folders.

mod common;

use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use common::{
    Env, HELP_LINE, TerminalRun, assert_quiet_success, config_with, in_terminal, in_terminal_sized,
    read_art, read_cache, stderr, stdout,
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

    /// The folder of the sized art.
    fn sizes(&self) -> PathBuf {
        self.config_folder().join("sizes")
    }

    /// The names of the files in the folder of the sized art, sorted.
    fn sized_art_names(&self) -> Vec<String> {
        let mut names: Vec<String> = fs::read_dir(self.sizes())
            .unwrap()
            .map(|entry| entry.unwrap().file_name().into_string().unwrap())
            .collect();
        names.sort();
        names
    }

    /// Runs `jaci banner` on a pseudo-terminal `columns` wide and `rows`
    /// high.
    fn banner_in(&self, columns: u16, rows: u16) -> TerminalRun {
        in_terminal_sized(self.command(&["banner"]), columns, rows)
    }

    /// The art file `jaci encode -w <columns> -o <tmp>/x.ansi` writes from
    /// the saved image with this env's config.
    fn encoded_aside(&self, columns: u32) -> Vec<u8> {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("x.ansi");
        let saved = self.config_folder().join("image.png");
        let output = self.jaci(&[
            "encode",
            "-w",
            &columns.to_string(),
            "-o",
            path.to_str().unwrap(),
            saved.to_str().unwrap(),
        ]);
        assert_quiet_success(&output);
        fs::read(path).unwrap()
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

/// [`set_up`]'s config folder after `jaci encode test_fixtures/4x4.png`
/// has run, so it holds a saved image.
fn with_saved_image(toml: &str) -> Env {
    let env = set_up(toml);
    let output = env.jaci(&["encode", "test_fixtures/4x4.png"]);
    assert_quiet_success(&output);
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
fn no_panel_runs_no_command() {
    let env = set_up(&config_with(&["{cmd:sleep 2}"]));
    let art = read_art(Path::new(HALFBLOCK));
    let rows = art.rows();
    // Narrower than the art, the default padding of 4 and the narrowest
    // panel of 20 columns, so no text panel shows.
    let columns = u16::try_from(art.columns() + 4 + 19).unwrap();

    let run = in_terminal_sized(env.command(&["banner"]), columns, 40);

    let expected: String = rows.iter().map(|row| format!("{row}\n")).collect();
    assert_shown(&run, &expected);
    let bound = Duration::from_secs(1);
    assert!(
        run.elapsed < bound,
        "the banner took {:?}: it ran the command with no panel to show",
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

#[test]
fn command_colour_ends_with_its_line() {
    const RED: &str = r#"{cmd:printf "\033[41mRED"}"#;
    for literal in ["", "x"] {
        let env = set_up(&config_with(&[&format!("{literal}{RED}")]));

        let run = env.banner(&["banner"]);

        let text = format!("{literal}\x1b[41mRED\x1b[0m");
        assert_shown(&run, &beside(Path::new(HALFBLOCK), &text));
    }
}

#[test]
fn new_variables_need_no_external_program() {
    let env = set_up(&config_with(&[
        "{ram_meter}|{disk_meter}|{date}|{time}|{week}",
    ]));
    let mut command = env.command(&["banner"]);
    command.env("PATH", "");

    let run = in_terminal_sized(command, 100, 40);

    assert_eq!(run.status.code(), Some(0), "{}", run.stderr);
    for shown in ["%", "GiB", "W"] {
        assert!(run.stdout.contains(shown), "no {shown}: {:?}", run.stdout);
    }
    assert!(!run.stdout.contains("unknown"), "{:?}", run.stdout);
}

#[test]
fn banner_reads_palette_every_run() {
    let env = Env::with_config("[[banner.text.blocks]]\nlabel = \"os\"\ncontent = \"x\"\n");
    env.add_cache(TESTHOST_CACHE);
    for (cell, label) in [
        (
            "\x1b[38;2;66;122;185m▀\x1b[0m\n",
            "\x1b[1m\x1b[38;2;66;122;185mos\x1b[0m",
        ),
        (
            "\x1b[38;2;204;110;77m▀\x1b[0m\n",
            "\x1b[1m\x1b[38;2;204;110;77mos\x1b[0m",
        ),
    ] {
        fs::write(env.art_path(), cell).unwrap();

        let run = in_terminal_sized(env.command(&["banner"]), 100, 40);

        assert_eq!(run.status.code(), Some(0), "{}", run.stderr);
        let first_row = run.stdout.lines().next().expect("the banner shows no rows");
        assert!(first_row.contains(label), "{first_row:?}");
    }
}

#[test]
fn title_needs_its_variables_in_cache() {
    let env = set_up("[[banner.text.blocks]]\nkind = \"title\"\n");

    let run = env.banner(&["banner"]);

    assert_setup_message(&run, "Almost there! Run 'jaci cache' to finish setting up.");
}

#[test]
fn banner_encodes_art_for_pane() {
    let env = with_saved_image(HOSTNAME_CONFIG);

    let run = env.banner_in(100, 40);

    let sized = env.sizes().join("60.ansi");
    assert_eq!(fs::read(&sized).unwrap(), env.encoded_aside(60));
    assert_eq!(read_art(&sized).rows().len(), 30);
    assert_shown(&run, &beside(&sized, "testhost"));
}

#[test]
fn kept_sized_art_shown_as_stored() {
    let env = with_saved_image(HOSTNAME_CONFIG);
    let first = env.banner_in(100, 40);
    assert_eq!(first.status.code(), Some(0), "{}", first.stderr);
    let row = format!("{}\x1b[0m", "x".repeat(60));
    fs::write(env.sizes().join("60.ansi"), format!("{row}\n")).unwrap();

    let run = env.banner_in(100, 40);

    assert_shown(&run, &format!("{row}    testhost\n"));
}

#[test]
fn sixteen_sized_arts_kept() {
    let env = with_saved_image("");

    for columns in (24..=39).chain([24, 40]) {
        let run = env.banner_in(columns, 40);
        assert_eq!(run.status.code(), Some(0), "{}", run.stderr);
    }

    let names = env.sized_art_names();
    assert_eq!(names.len(), 16, "{names:?}");
    for kept in ["20.ansi", "36.ansi"] {
        assert!(
            names.iter().any(|name| name == kept),
            "no {kept}: {names:?}"
        );
    }
    assert!(!env.sizes().join("21.ansi").exists(), "{names:?}");
}

#[test]
fn changed_encode_settings_drop_sized_art() {
    let env = with_saved_image(HOSTNAME_CONFIG);
    let first = env.banner_in(50, 40);
    assert_eq!(first.status.code(), Some(0), "{}", first.stderr);
    assert!(env.sizes().join("38.ansi").exists());
    fs::write(
        env.config_path(),
        format!("{HOSTNAME_CONFIG}\n[encode]\nmode = \"halfblock\"\n"),
    )
    .unwrap();

    let run = env.banner_in(100, 40);

    assert_eq!(run.status.code(), Some(0), "{}", run.stderr);
    assert_eq!(env.sized_art_names(), ["60.ansi"]);
    assert_eq!(
        fs::read(env.sizes().join("60.ansi")).unwrap(),
        env.encoded_aside(60)
    );
}

#[test]
fn without_saved_image_art_file_shown_as_today() {
    let env = set_up(HOSTNAME_CONFIG);

    let run = env.banner_in(30, 3);

    let art = read_art(Path::new(HALFBLOCK));
    let rows = art.rows();
    assert_eq!(rows.len(), 2);
    assert_shown(&run, &format!("{}    testhost\n{}\n", rows[0], rows[1]));
    assert!(!env.sizes().exists());
}

#[test]
fn art_argument_shown_as_today() {
    let env = with_saved_image(HOSTNAME_CONFIG);
    let tmp = TempDir::new().unwrap();
    let other = tmp.path().join("other.ansi");
    fs::copy(ASCII, &other).unwrap();

    let run = in_terminal_sized(env.command(&["banner", other.to_str().unwrap()]), 100, 40);

    assert_shown(&run, &beside(&other, "testhost"));
    assert!(!env.sizes().exists());
}

#[test]
fn pane_of_unknown_size_shows_todays_banner() {
    let env = with_saved_image(HOSTNAME_CONFIG);

    let run = env.banner_in(0, 0);

    let art = read_art(&env.art_path());
    let expected: String = art.rows().iter().map(|row| format!("{row}\n")).collect();
    assert_shown(&run, &expected);
    assert!(!env.sizes().exists());
}

#[test]
fn help_tells_of_the_layout_and_the_saved_image() {
    let output = Env::new().jaci(&["banner", "--help"]);

    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    let help = stdout(&output);
    assert!(help.contains("laid out for the pane"), "{help}");
    assert!(help.contains("sized from the saved image"), "{help}");
    assert!(help.contains("instead of the sized art"), "{help}");
}
