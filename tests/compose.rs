mod common;

use common::banner;
use jaci::compose::{self, Layout, Pane, Panel};
use jaci::config::{Align, Encode, Valign};
use jaci::size::{self, Sizing};

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

/// The padding before the panel in the layout scenarios.
const PADDING: u32 = 4;
/// The saved image of the layout scenarios: 100×100 pixels.
const IMAGE: (u32, u32) = (100, 100);

/// `[encode]` with size limits 20 and 60 and `ratio` 0.5.
fn limits() -> Encode {
    Encode {
        min_width: 20,
        max_width: 60,
        ratio: 0.5,
        ..Encode::default()
    }
}

/// `count` plain art rows, each `columns` wide and distinct.
fn plain_art(columns: usize, count: usize) -> Vec<String> {
    (0..count)
        .map(|row| format!("{}{}", row % 10, "#".repeat(columns - 1)))
        .collect()
}

/// `count` panel lines, the first `widest` columns wide and the rest
/// narrower.
fn panel_lines(count: usize, widest: usize) -> Vec<String> {
    (0..count)
        .map(|line| {
            if line == 0 {
                "x".repeat(widest)
            } else {
                format!("line {line}")
            }
        })
        .collect()
}

/// The layout of the scenarios' saved image beside `text` in a pane
/// `columns` by `rows`.
fn layout_in(columns: usize, rows: usize, text: &[String]) -> Layout {
    compose::layout(Pane { columns, rows }, &limits(), PADDING, IMAGE, text)
}

/// The rows art `columns` wide takes of the scenarios' saved image with
/// [`limits`], as `jaci encode -w <columns>` sizes it.
fn art_rows(columns: usize) -> u32 {
    let encode = limits();
    let image = image::RgbaImage::new(IMAGE.0, IMAGE.1);
    let sizing = Sizing {
        width: Some(u32::try_from(columns).unwrap()),
        config_ratio: Some(encode.ratio),
        ..Sizing::default()
    };
    size::fit(&image, encode.mode, &sizing).unwrap().rows
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

#[test]
fn side_by_side_takes_largest_art_up_to_maximum() {
    let text = panel_lines(5, 30);

    let layout = layout_in(100, 40, &text);

    let Layout::SideBySide { art_columns, panel } = layout else {
        panic!("not side by side: {layout:?}");
    };
    assert_eq!(art_columns, 60);
    assert_eq!(panel.width(), 36);
    let art = plain_art(60, 30);
    let rows = compose::rows(layout, &art, &text, Align::Left, Valign::Top);
    assert_eq!(rows.len(), 30);
    assert_eq!(rows[0], format!("{}    {}", art[0], text[0]));
    assert_eq!(rows[5], art[5]);
}

#[test]
fn side_by_side_leaves_room_for_widest_line() {
    let layout = layout_in(80, 40, &panel_lines(5, 30));

    let Layout::SideBySide { art_columns, panel } = layout else {
        panic!("not side by side: {layout:?}");
    };
    assert_eq!(art_columns, 46);
    assert_eq!(art_rows(art_columns), 23);
    assert_eq!(panel.width(), 30);
}

#[test]
fn side_by_side_leaves_two_rows_for_prompt() {
    let layout = layout_in(120, 24, &panel_lines(5, 30));

    let Layout::SideBySide { art_columns, .. } = layout else {
        panic!("not side by side: {layout:?}");
    };
    // 45 columns would take round(22.5) = 23 rows.
    assert_eq!(art_columns, 44);
    assert_eq!(art_rows(art_columns), 22);
}

#[test]
fn panel_centred_below_art() {
    let text = panel_lines(8, 30);

    let layout = layout_in(50, 60, &text);

    assert_eq!(
        layout,
        Layout::PanelBelow {
            art_columns: 50,
            panel_width: 30
        }
    );
    let art = plain_art(50, 25);
    let rows = compose::rows(layout, &art, &text, Align::Left, Valign::Top);
    let mut expected = art.clone();
    expected.push(String::new());
    expected.extend(text.iter().map(|line| format!("{}{line}", " ".repeat(10))));
    assert_eq!(rows, expected);
}

#[test]
fn art_centred_above_wider_panel() {
    let text = vec!["x".repeat(40), "y".to_owned()];

    let layout = layout_in(44, 20, &text);

    assert_eq!(
        layout,
        Layout::PanelBelow {
            art_columns: 30,
            panel_width: 40
        }
    );
    let art = plain_art(30, 15);
    let rows = compose::rows(layout, &art, &text, Align::Left, Valign::Top);
    let mut expected: Vec<String> = art.iter().map(|row| format!("     {row}")).collect();
    expected.push(String::new());
    expected.extend(text.iter().cloned());
    assert_eq!(rows, expected);
}

#[test]
fn art_alone_when_panel_below_does_not_fit() {
    let text = panel_lines(8, 30);

    let layout = layout_in(50, 20, &text);

    assert_eq!(layout, Layout::ArtAlone { art_columns: 36 });
    let art = plain_art(36, 18);
    let rows = compose::rows(layout, &art, &text, Align::Left, Valign::Top);
    assert_eq!(rows, art);
}

#[test]
fn panel_alone_in_pane_narrower_than_minimum_art() {
    let text = vec!["x".repeat(25)];

    let layout = layout_in(18, 20, &text);

    assert_eq!(layout, Layout::PanelAlone { panel_width: 18 });
    let rows = compose::rows(layout, &[], &text, Align::Left, Valign::Top);
    assert_eq!(rows, ["x".repeat(18), "x".repeat(7)]);
}

#[test]
fn panel_alone_when_minimum_art_too_tall() {
    let text = lines(&["a", "b"]);

    let layout = layout_in(80, 11, &text);

    assert_eq!(layout, Layout::PanelAlone { panel_width: 80 });
    let rows = compose::rows(layout, &[], &text, Align::Left, Valign::Top);
    assert_eq!(rows, ["a", "b"]);
}

#[test]
fn wide_character_in_one_column_pane_starts_at_its_left_edge() {
    let text = lines(&["a界"]);

    let layout = layout_in(1, 20, &text);

    assert_eq!(layout, Layout::PanelAlone { panel_width: 1 });
    for align in [Align::Center, Align::Right] {
        let rows = compose::rows(layout, &[], &text, align, Valign::Top);
        assert_eq!(rows, ["a", "界"], "{align:?}");
    }
}

#[test]
fn no_panel_lines_show_art_alone() {
    let layout = layout_in(22, 40, &[]);

    assert_eq!(layout, Layout::ArtAlone { art_columns: 22 });
    let art = plain_art(22, 11);
    let rows = compose::rows(layout, &art, &[], Align::Left, Valign::Top);
    assert_eq!(rows, art);
}

#[test]
fn only_empty_panel_lines_show_art_alone() {
    let text = lines(&["", ""]);

    let layout = layout_in(22, 40, &text);

    assert_eq!(layout, Layout::ArtAlone { art_columns: 22 });
    let art = plain_art(22, 11);
    let rows = compose::rows(layout, &art, &text, Align::Left, Valign::Top);
    assert_eq!(rows, art);
}
