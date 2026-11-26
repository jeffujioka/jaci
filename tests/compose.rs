mod common;

use common::banner;
use jaci::compose::{self, Panel};
use jaci::config::{Align, Valign};

/// The width of every [`art`] row.
const ART_COLUMNS: usize = 10;

/// `count` art rows, each [`ART_COLUMNS`] wide: five cells of two half
/// blocks in a 24-bit colour, ending in a reset, so the width skips real
/// escapes.
fn art(count: usize) -> Vec<String> {
    (0..count)
        .map(|row| {
            format!(
                "\x1b[38;2;{row};20;30m\x1b[48;2;40;50;60m{}\x1b[0m",
                "▀▄".repeat(5)
            )
        })
        .collect()
}

/// The given text lines as owned strings.
fn lines(lines: &[&str]) -> Vec<String> {
    lines.iter().map(|&line| line.to_owned()).collect()
}

#[test]
fn text_sits_beside_art() {
    let art = art(3);

    let rows = banner(
        &art,
        ART_COLUMNS,
        &lines(&["a", "b"]),
        4,
        Align::Left,
        Valign::Top,
        60,
    );

    assert_eq!(
        rows,
        [
            format!("{}    a", art[0]),
            format!("{}    b", art[1]),
            art[2].clone(),
        ]
    );
}

#[test]
fn narrow_terminal_shows_art_only() {
    let art = art(2);

    let rows = banner(
        &art,
        ART_COLUMNS,
        &lines(&["a"]),
        4,
        Align::Left,
        Valign::Top,
        33,
    );

    assert_eq!(rows, art);
}

#[test]
fn panel_is_whats_left_and_at_least_20_columns() {
    let width = |art_columns, padding, terminal_width| {
        compose::panel_width(art_columns, padding, terminal_width).map(Panel::width)
    };
    assert_eq!(width(10, 4, 34), Some(20));
    assert_eq!(width(10, 4, 100), Some(86));
    assert_eq!(width(10, 4, 33), None);
    assert_eq!(width(10, 4, 0), None);
    assert_eq!(width(10, u32::MAX, 100), None);
}

#[test]
fn text_taller_than_art_continues_below() {
    let art = art(1);

    let rows = banner(
        &art,
        ART_COLUMNS,
        &lines(&["a", "b", "c"]),
        4,
        Align::Left,
        Valign::Top,
        60,
    );

    assert_eq!(
        rows,
        [
            format!("{}    a", art[0]),
            format!("{}b", " ".repeat(14)),
            format!("{}c", " ".repeat(14)),
        ]
    );
}

#[test]
fn long_line_wraps_inside_panel() {
    let art = art(2);
    let line = "x".repeat(25);

    let rows = banner(&art, ART_COLUMNS, &[line], 4, Align::Left, Valign::Top, 34);

    assert_eq!(
        rows,
        [
            format!("{}    {}", art[0], "x".repeat(20)),
            format!("{}    {}", art[1], "x".repeat(5)),
        ]
    );
}

#[test]
fn wide_characters_count_by_display_width() {
    let art = art(2);
    let line = "日".repeat(12);

    let rows = banner(&art, ART_COLUMNS, &[line], 4, Align::Left, Valign::Top, 34);

    assert_eq!(
        rows,
        [
            format!("{}    {}", art[0], "日".repeat(10)),
            format!("{}    {}", art[1], "日".repeat(2)),
        ]
    );
}

#[test]
fn right_alignment() {
    let art = art(1);

    let rows = banner(
        &art,
        ART_COLUMNS,
        &lines(&["abc"]),
        4,
        Align::Right,
        Valign::Top,
        34,
    );

    assert_eq!(rows, [format!("{}    {}abc", art[0], " ".repeat(17))]);
}

#[test]
fn centre_alignment() {
    let art = art(1);

    let rows = banner(
        &art,
        ART_COLUMNS,
        &lines(&["abc"]),
        4,
        Align::Center,
        Valign::Top,
        34,
    );

    assert_eq!(rows, [format!("{}    {}abc", art[0], " ".repeat(8))]);
}

#[test]
fn bottom_alignment() {
    let art = art(4);

    let rows = banner(
        &art,
        ART_COLUMNS,
        &lines(&["a"]),
        4,
        Align::Left,
        Valign::Bottom,
        60,
    );

    assert_eq!(
        rows,
        [
            art[0].clone(),
            art[1].clone(),
            art[2].clone(),
            format!("{}    a", art[3]),
        ]
    );
}

#[test]
fn middle_alignment() {
    let art = art(5);

    let rows = banner(
        &art,
        ART_COLUMNS,
        &lines(&["a", "b"]),
        4,
        Align::Left,
        Valign::Center,
        60,
    );

    assert_eq!(
        rows,
        [
            art[0].clone(),
            format!("{}    a", art[1]),
            format!("{}    b", art[2]),
            art[3].clone(),
            art[4].clone(),
        ]
    );
}

#[test]
fn wrapped_styled_line_keeps_its_style_on_every_piece() {
    let art = art(1);
    let line = format!("\x1b[1m\x1b[38;2;1;2;3m{}\x1b[0m", "y".repeat(25));

    let rows = banner(&art, ART_COLUMNS, &[line], 4, Align::Right, Valign::Top, 34);

    assert_eq!(
        rows,
        [
            format!(
                "{}    \x1b[1m\x1b[38;2;1;2;3m{}\x1b[0m",
                art[0],
                "y".repeat(20)
            ),
            format!(
                "{}\x1b[1m\x1b[38;2;1;2;3m{}\x1b[0m",
                " ".repeat(14 + 15),
                "y".repeat(5)
            ),
        ]
    );
}

#[test]
fn wide_character_that_does_not_fit_moves_to_next_row_whole() {
    let art = art(2);
    let line = "日".repeat(11);

    // A panel of 21 columns holds 10 wide characters and one free column.
    let rows = banner(&art, ART_COLUMNS, &[line], 4, Align::Left, Valign::Top, 35);

    assert_eq!(
        rows,
        [
            format!("{}    {}", art[0], "日".repeat(10)),
            format!("{}    日", art[1]),
        ]
    );
}

#[test]
fn empty_text_line_adds_no_trailing_spaces() {
    let art = art(2);

    let rows = banner(
        &art,
        ART_COLUMNS,
        &lines(&["", "", "b"]),
        4,
        Align::Center,
        Valign::Top,
        60,
    );

    assert_eq!(
        rows,
        [
            art[0].clone(),
            art[1].clone(),
            format!("{}b", " ".repeat(14 + 22)),
        ]
    );
}

#[test]
fn colour_set_mid_line_carries_to_every_later_piece() {
    let art = art(1);
    let line = format!(
        "\x1b[1m{}\x1b[31m{}\x1b[32m{}\x1b[0m",
        "a".repeat(15),
        "b".repeat(10),
        "c".repeat(20)
    );

    let rows = banner(&art, ART_COLUMNS, &[line], 4, Align::Left, Valign::Top, 34);

    let indent = " ".repeat(14);
    assert_eq!(
        rows,
        [
            format!(
                "{}    \x1b[1m{}\x1b[31m{}\x1b[0m",
                art[0],
                "a".repeat(15),
                "b".repeat(5)
            ),
            format!(
                "{indent}\x1b[1m\x1b[31m{}\x1b[32m{}\x1b[0m",
                "b".repeat(5),
                "c".repeat(15)
            ),
            format!("{indent}\x1b[1m\x1b[31m\x1b[32m{}\x1b[0m", "c".repeat(5)),
        ]
    );
}

#[test]
fn unstyled_line_with_a_colour_ends_with_a_reset() {
    let art = art(2);
    let line = format!("{}\x1b[41m{}", "a".repeat(15), "b".repeat(10));

    let rows = banner(&art, ART_COLUMNS, &[line], 4, Align::Left, Valign::Top, 34);

    assert_eq!(
        rows,
        [
            format!(
                "{}    {}\x1b[41m{}\x1b[0m",
                art[0],
                "a".repeat(15),
                "b".repeat(5)
            ),
            format!("{}    \x1b[41m{}\x1b[0m", art[1], "b".repeat(5)),
        ]
    );
}

#[test]
fn piece_of_only_escapes_shows_no_text() {
    let art = art(1);

    let rows = banner(
        &art,
        ART_COLUMNS,
        &lines(&["\x1b[41m"]),
        4,
        Align::Left,
        Valign::Top,
        34,
    );

    assert_eq!(rows, [art[0].clone()]);
}

#[test]
fn art_row_with_an_escape_and_no_reset_ends_with_one() {
    let art = lines(&["\x1b[41m  ", "\x1b[42m  ", "##"]);

    let rows = banner(&art, 2, &lines(&["a"]), 4, Align::Left, Valign::Top, 60);
    let narrow = banner(&art, 2, &lines(&["a"]), 4, Align::Left, Valign::Top, 10);

    assert_eq!(rows, ["\x1b[41m  \x1b[0m    a", "\x1b[42m  \x1b[0m", "##"]);
    assert_eq!(narrow, ["\x1b[41m  \x1b[0m", "\x1b[42m  \x1b[0m", "##"]);
}

#[test]
fn art_row_without_an_escape_is_shown_as_stored() {
    let art = lines(&["##", "##"]);

    let rows = banner(&art, 2, &lines(&["a"]), 4, Align::Left, Valign::Top, 60);

    assert_eq!(rows, ["##    a", "##"]);
}

#[test]
fn reset_restarts_the_style_a_continuation_replays() {
    let art = art(1);
    let line = format!("\x1b[1m{}\x1b[0m\x1b[32m{}", "a".repeat(15), "b".repeat(10));

    let rows = banner(&art, ART_COLUMNS, &[line], 4, Align::Left, Valign::Top, 34);

    assert_eq!(
        rows,
        [
            format!(
                "{}    \x1b[1m{}\x1b[0m\x1b[32m{}\x1b[0m",
                art[0],
                "a".repeat(15),
                "b".repeat(5)
            ),
            format!("{}\x1b[32m{}\x1b[0m", " ".repeat(14), "b".repeat(5)),
        ]
    );
}
