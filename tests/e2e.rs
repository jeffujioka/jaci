//! jaci end to end: `jaci init` on an image, then `jaci banner` on a
//! pseudo-terminal, both as the built binary with their config folder and
//! home folder in temporary folders.

mod common;

use common::{Env, in_terminal_sized, plain, read_art, read_cache, stderr};
use jaci::sysinfo::StaticVariable::{Hostname, User};

/// Wide and high enough for the widest art, 60 columns by 30 rows, beside
/// the full config's text panel.
const COLUMNS: u16 = 200;
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

    let art = read_art(&env.config_folder().join("sizes").join("60.ansi"));
    let art_rows = art.rows();
    assert_eq!(art_rows.len(), 30);
    let lines: Vec<&str> = banner.stdout.lines().collect();
    assert_eq!(lines.len(), art_rows.len(), "banner: {}", banner.stdout);
    for (line, art_row) in lines.iter().zip(art_rows) {
        assert!(
            line.starts_with(art_row.as_str()),
            "{line:?} starts with art row {art_row:?}"
        );
    }

    let cache = read_cache(&env.cache_path());
    let title = format!(
        "{}@{}",
        cache.get(User).unwrap(),
        cache.get(Hostname).unwrap()
    );
    let texts: Vec<String> = lines
        .iter()
        .zip(art_rows)
        .map(|(line, art_row)| plain(&line[art_row.len()..]))
        .collect();
    let title_row = texts
        .iter()
        .position(|text| text.contains(&title))
        .unwrap_or_else(|| panic!("no row shows {title}: {texts:#?}"));
    assert!(
        texts[title_row + 1..].iter().any(|text| {
            text.contains("ram")
                && text.split_whitespace().any(|word| {
                    word.strip_suffix('%')
                        .is_some_and(|number| number.parse::<u8>().is_ok())
                })
        }),
        "no later row shows ram and a percentage: {texts:#?}"
    );
}
