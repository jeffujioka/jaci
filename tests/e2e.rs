//! jaci end to end: `jaci init` on an image, then `jaci banner` on a
//! pseudo-terminal, both as the built binary with their config folder and
//! home folder in temporary folders.

mod common;

use common::{Env, in_terminal_sized, read_art, stderr};

/// Wide enough for the 80-column art, the padding and the full config's
/// text panel.
const COLUMNS: u16 = 120;
const ROWS: u16 = 50;

#[test]
fn banner_works_right_after_init() {
    let env = Env::new();

    let init = env.jaci(&["init", "test_fixtures/4x4.png"]);
    assert!(init.status.success(), "init: {init:?}");
    assert_eq!(stderr(&init), "");

    let banner = in_terminal_sized(env.command(&["banner"]), COLUMNS, ROWS);
    assert!(banner.status.success(), "banner: {banner:?}");
    assert_eq!(banner.stderr, "");

    let config = env.config();
    let art = read_art(&config.banner.image);
    let art_rows = art.rows();
    assert_eq!(art_rows.len(), 40);
    let lines: Vec<&str> = banner.stdout.lines().collect();
    assert_eq!(lines.len(), art_rows.len(), "banner: {}", banner.stdout);
    for (line, art_row) in lines.iter().zip(art_rows) {
        assert!(
            line.starts_with(art_row.as_str()),
            "{line:?} starts with art row {art_row:?}"
        );
    }

    let padding = " ".repeat(usize::try_from(config.banner.padding).unwrap());
    let text = lines[0]
        .strip_prefix(art_rows[0].as_str())
        .and_then(|rest| rest.strip_prefix(padding.as_str()))
        .unwrap_or_else(|| panic!("row 1 has the art and the padding: {:?}", lines[0]));
    let host = text
        .strip_prefix("Host: ")
        .unwrap_or_else(|| panic!("row 1 shows the host: {text:?}"));
    assert!(!host.trim().is_empty(), "row 1 names the host: {text:?}");
}
