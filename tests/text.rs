mod common;

use std::path::Path;
use std::thread;
use std::time::{Duration, Instant};

use common::{content_block, read_cache};
use jaci::cache::{self, Cache};
use jaci::config::{Align, Block, BlockText, Style, Valign};
use jaci::sysinfo::StaticVariable::Hostname;
use jaci::{compose, text};
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

#[test]
fn command_output_keeps_its_colours() {
    let lines = text::resolve(
        &[command(r"printf '\033[1;31mr\033[mx\033[0m'")],
        &Cache::default(),
    );

    assert_eq!(lines, ["\x1b[1;31mr\x1b[mx\x1b[0m"]);
}

#[test]
fn tabs_in_command_output_expand_to_8_column_stops() {
    let blocks = [
        content_block(r"x{cmd:printf 'a\tb'}y"),
        command(r"printf 'a\tb\n日\tc\n\td'"),
    ];

    let lines = text::resolve(&blocks, &Cache::default());

    assert_eq!(
        lines,
        [
            format!("xa{}by", " ".repeat(6)),
            format!("a{}b", " ".repeat(7)),
            format!("日{}c", " ".repeat(6)),
            format!("{}d", " ".repeat(8)),
        ]
    );
    let art = vec!["#".repeat(10)];
    let rows = compose::banner(&art, &lines[..1], 4, Align::Right, Valign::Top, 34);
    assert_eq!(
        rows,
        [format!("{}    {}{}", art[0], " ".repeat(10), lines[0])]
    );
}

#[test]
fn carriage_return_ending_literal_content_is_dropped() {
    let lines = text::resolve(&[content_block("a\r")], &Cache::default());

    assert_eq!(lines, ["a"]);
}

#[test]
fn command_output_controls_and_other_escapes_are_removed() {
    let output = [
        r"a\rb",
        r"\033[2Jc",
        r"\033]0;x\007d",
        r"\033]0;y\033\\e",
        r"\033(Bf\033",
        r"7g\001\010\177h\033",
    ]
    .concat();

    let lines = text::resolve(&[command(&format!("printf '{output}'"))], &Cache::default());

    assert_eq!(lines, ["abcdefgh"]);
}

#[test]
fn literal_controls_and_other_escapes_are_removed() {
    let blocks = [
        content_block("a\rb"),
        content_block("\x1b[2Jc\u{8}d\u{7f}\u{9b}e\x1b]0;t\x07f"),
        content_block("x\x1b[31my"),
    ];

    let lines = text::resolve(&blocks, &Cache::default());

    assert_eq!(lines, ["ab", "cdef", "x\x1b[31my"]);
    let art = vec!["#".repeat(10)];
    let rows = compose::banner(&art, &lines[2..], 4, Align::Left, Valign::Top, 34);
    assert_eq!(rows, [format!("{}    x\x1b[31my\x1b[0m", art[0])]);
}

#[test]
fn command_output_c1_controls_and_control_strings_are_removed() {
    let output = [
        // C1 controls: U+0085, U+009B and U+009F.
        r"a\302\205b\302\233c\302\237",
        // DCS, SOS, PM and APC strings, ended by ST or BEL.
        r"d\033Pq#0;1\033\\e",
        r"\033Xsos\007f",
        r"\033^pm\033\\g",
        r"\033_apc\007h",
        // Two-character escapes.
        r"\0337i\033cj\033=k\0338",
        // A control string to the end of the output.
        r"l\033Pnever ended",
    ]
    .concat();

    let lines = text::resolve(&[command(&format!("printf '{output}'"))], &Cache::default());

    assert_eq!(lines, ["abcdefghijkl"]);
}

#[test]
fn tabs_in_literal_content_expand_to_8_column_stops() {
    let cache = [(Hostname, "testhost".to_owned())].into_iter().collect();
    let blocks = [
        content_block("Host:\t{hostname}\tx"),
        content_block("{cmd:printf ab}\tc\n\td"),
    ];

    let lines = text::resolve(&blocks, &cache);

    assert_eq!(
        lines,
        [
            format!("Host:{}testhost{}x", " ".repeat(3), " ".repeat(8)),
            format!("ab{}c", " ".repeat(6)),
            format!("{}d", " ".repeat(8)),
        ]
    );
}

#[test]
fn colour_carries_across_the_lines_of_one_block() {
    let blocks = [
        command(r"printf '\033[36ma\nb\n\033[m'"),
        content_block("c"),
    ];

    let lines = text::resolve(&blocks, &Cache::default());

    assert_eq!(lines, ["\x1b[36ma", "\x1b[36mb", "c"]);
    let art = vec!["\x1b[41m  \x1b[0m".to_owned(); 2];
    let rows = compose::banner(&art, &lines[..2], 4, Align::Left, Valign::Top, 30);
    assert_eq!(
        rows,
        [
            format!("{}    \x1b[36ma\x1b[0m", art[0]),
            format!("{}    \x1b[36mb\x1b[0m", art[1]),
        ]
    );
}

#[test]
fn colour_carries_after_a_styled_block_prefix() {
    let block = Block {
        style: Style::Bold,
        ..command(r"printf '\033[31ma\nb'")
    };

    let lines = text::resolve(&[block], &Cache::default());

    assert_eq!(
        lines,
        ["\x1b[1m\x1b[31ma\x1b[0m", "\x1b[1m\x1b[31mb\x1b[0m"]
    );
}

#[test]
fn reset_ends_the_carried_colour() {
    let blocks = [
        command(r"printf '\033[1mx\033[0m\ny'"),
        command(r"printf '\033[1;32mz\033[m\033[4mw\nv'"),
    ];

    let lines = text::resolve(&blocks, &Cache::default());

    assert_eq!(
        lines,
        [
            "\x1b[1mx\x1b[0m",
            "y",
            "\x1b[1;32mz\x1b[m\x1b[4mw",
            "\x1b[4mv",
        ]
    );
}

#[test]
fn trailing_escape_only_lines_of_output_are_dropped() {
    let blocks = [
        command(r"printf 'a\n\033(B\033[m\n'"),
        content_block(r"g{cmd:printf 'h\n\033[m'}"),
        command(r"printf 'e\n\n\033[m'"),
    ];

    let lines = text::resolve(&blocks, &Cache::default());

    assert_eq!(lines, ["a", "gh", "e", ""]);
}

#[test]
fn literal_newline_after_output_does_not_keep_its_last_line() {
    let blocks = [
        content_block(concat!(r"{cmd:printf 'h\n\033[m'}", "\n")),
        content_block("b\n\n"),
    ];

    let lines = text::resolve(&blocks, &Cache::default());

    assert_eq!(lines, ["h", "b", ""]);
}

#[test]
fn literal_lines_are_never_dropped() {
    let blocks = [
        content_block("b\n\n"),
        content_block("{cmd:printf c}\n\x1b[1m"),
    ];

    let lines = text::resolve(&blocks, &Cache::default());

    assert_eq!(lines, ["b", "", "c", "\x1b[1m"]);
}
