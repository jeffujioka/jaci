mod common;

use std::path::Path;
use std::thread;
use std::time::{Duration, Instant};

use common::{content_block, read_cache};
use jaci::cache::{self, Cache};
use jaci::config::{Block, BlockText, Style};
use jaci::sysinfo::StaticVariable::Hostname;
use jaci::text;
use tempfile::TempDir;

/// A normal block without colour whose `command` is `command`.
fn command(command: &str) -> Block {
    Block {
        text: BlockText::Command(command.to_owned()),
        style: Style::Normal,
        color: None,
    }
}

/// Writes `cache` to a cache file in `folder` and reads it back.
fn through_file(folder: &Path, cache: &Cache) -> Cache {
    let path = folder.join(cache::FILE_NAME);
    cache::write(&path, cache).unwrap();
    read_cache(&path)
}

#[test]
fn static_variable_from_cache_file() {
    let folder = TempDir::new().unwrap();
    let cache = through_file(
        folder.path(),
        &[(Hostname, "testhost".to_owned())].into_iter().collect(),
    );

    let lines = text::resolve(&[content_block("Host: {hostname}")], &cache);

    assert_eq!(lines, ["Host: testhost"]);
}

#[test]
fn dynamic_variable_read_fresh() {
    let folder = TempDir::new().unwrap();
    let cache = through_file(folder.path(), &Cache::default());

    let lines = text::resolve(&[content_block("{uptime}")], &cache);

    assert_eq!(lines.len(), 1, "{lines:?}");
    assert!(!lines[0].is_empty());
    assert_ne!(lines[0], "{uptime}");
}

#[test]
fn unknown_placeholder_stays() {
    let lines = text::resolve(&[content_block("{weather} today")], &Cache::default());

    assert_eq!(lines, ["{weather} today"]);
}

#[test]
fn inline_command_output() {
    let lines = text::resolve(&[content_block("Say: {cmd:echo hi}")], &Cache::default());

    assert_eq!(lines, ["Say: hi"]);
}

#[test]
fn nested_braces_stay_inside_command() {
    let lines = text::resolve(
        &[content_block("{cmd:echo a b | awk '{print $1}'}!")],
        &Cache::default(),
    );

    assert_eq!(lines, ["a!"]);
}

#[test]
fn block_command_one_line_per_output_line() {
    let lines = text::resolve(&[command("printf 'x\\ny\\n'")], &Cache::default());

    assert_eq!(lines, ["x", "y"]);
}

#[test]
fn failed_command_shows_unknown() {
    let lines = text::resolve(
        &[
            content_block("{cmd:exit 3}"),
            command("no-such-command-jaci"),
        ],
        &Cache::default(),
    );

    assert_eq!(lines, ["unknown", "unknown"]);
}

#[test]
fn slow_command_shows_unknown() {
    let start = Instant::now();

    let lines = text::resolve(
        &[
            content_block("{cmd:sleep 10}"),
            content_block("{cmd:echo hi}"),
        ],
        &Cache::default(),
    );

    assert!(
        start.elapsed() < Duration::from_secs(4),
        "{:?}",
        start.elapsed()
    );
    assert_eq!(lines, ["unknown", "hi"]);
}

#[test]
fn commands_run_in_parallel() {
    let block = content_block("{cmd:sleep 1; echo x}");
    let start = Instant::now();

    let lines = text::resolve(&[block.clone(), block.clone(), block], &Cache::default());

    assert!(
        start.elapsed() < Duration::from_millis(2500),
        "{:?}",
        start.elapsed()
    );
    assert_eq!(lines, ["x", "x", "x"]);
}

#[test]
fn timed_out_command_is_stopped() {
    let folder = TempDir::new().unwrap();
    let late = folder.path().join("late");
    let block = content_block(&format!("{{cmd:sleep 4; touch '{}'}}", late.display()));
    let start = Instant::now();

    let lines = text::resolve(&[block], &Cache::default());
    thread::sleep(Duration::from_secs(5).saturating_sub(start.elapsed()));

    assert_eq!(lines, ["unknown"]);
    assert!(!late.exists(), "{} exists", late.display());
}

/// Guards the ruling that a timed-out command's whole process group is
/// killed, not only its shell: the background subshell outlives a shell-only
/// kill and would create `late`.
#[test]
fn timed_out_command_group_is_killed() {
    let folder = TempDir::new().unwrap();
    let late = folder.path().join("late");
    let block = content_block(&format!(
        "{{cmd:(sleep 4; touch '{}') & sleep 10}}",
        late.display()
    ));
    let start = Instant::now();

    let lines = text::resolve(&[block], &Cache::default());
    thread::sleep(Duration::from_secs(5).saturating_sub(start.elapsed()));

    assert_eq!(lines, ["unknown"]);
    assert!(!late.exists(), "{} exists", late.display());
}

/// Guards the time limit when a command closes its stdout but keeps running.
#[test]
fn command_closing_stdout_keeps_time_limit() {
    let start = Instant::now();

    let lines = text::resolve(
        &[content_block("{cmd:exec >&-; sleep 30}")],
        &Cache::default(),
    );

    assert!(
        start.elapsed() < Duration::from_secs(4),
        "{:?}",
        start.elapsed()
    );
    assert_eq!(lines, ["unknown"]);
}

#[test]
fn bold_coloured_block() {
    let block = Block {
        style: Style::Bold,
        color: Some([0xFF, 0x88, 0x00]),
        ..content_block("hi")
    };

    let lines = text::resolve(&[block], &Cache::default());

    assert_eq!(lines, ["\x1b[1m\x1b[38;2;255;136;0mhi\x1b[0m"]);
}

#[test]
fn plain_block_has_no_escapes() {
    let lines = text::resolve(&[content_block("hi")], &Cache::default());

    assert_eq!(lines, ["hi"]);
}
