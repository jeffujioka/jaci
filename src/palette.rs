//! The art palette: the few saturated colours of an art file, which colour
//! the text panel.

use std::cmp::Reverse;
use std::collections::HashMap;

use crate::ansi::{self, Token};
use crate::art_file::Art;

/// The palette when the art has no colour left: the palette of the
/// developer's art.
const BUILT_IN: [[u8; 3]; 6] = [
    [0xCC, 0x6E, 0x4D],
    [0xE3, 0xCE, 0x7B],
    [0xC6, 0xDB, 0x7A],
    [0x94, 0xCF, 0x77],
    [0x68, 0x9E, 0xC1],
    [0x42, 0x7A, 0xB9],
];
/// The most colours a palette holds.
const MOST_COLOURS: usize = 6;
/// The hue groups, 30° of HSV hue each from 0° (red).
const HUE_GROUPS: usize = 12;
/// A colour whose largest channel less its smallest is under this is grey.
const GREY_SPREAD: u8 = 64;
/// A colour whose largest channel is under this is dark.
const DARK_LARGEST: u8 = 128;

/// One colour's record: how often the art uses it, and where it first does.
#[derive(Clone, Copy)]
struct Uses {
    colour: [u8; 3],
    count: usize,
    first: usize,
}

/// The palette of `art`: its 24-bit colours without grey and dark ones, the
/// most used colour of each of the six most used hue groups, ordered by hue;
/// the built-in set when no colour is left.
pub fn of(art: &Art) -> Vec<[u8; 3]> {
    let mut uses: HashMap<[u8; 3], Uses> = HashMap::new();
    let found = art
        .rows()
        .iter()
        .flat_map(|row| ansi::tokens(row))
        .filter_map(|token| match token {
            Token::Escape(escape) => Some(ansi::colours(escape)),
            Token::Char(_) => None,
        })
        .flatten()
        .filter(|&colour| !is_grey_or_dark(colour));
    for (index, colour) in found.enumerate() {
        uses.entry(colour)
            .or_insert(Uses {
                colour,
                count: 0,
                first: index,
            })
            .count += 1;
    }

    let mut groups: [(usize, Option<Uses>); HUE_GROUPS] = [(0, None); HUE_GROUPS];
    for colour in uses.into_values() {
        let (total, best) = &mut groups[hue_group(colour.colour)];
        *total += colour.count;
        if best.is_none_or(|best| (colour.count, best.first) > (best.count, colour.first)) {
            *best = Some(colour);
        }
    }

    let mut kept: Vec<usize> = (0..HUE_GROUPS)
        .filter(|&group| groups[group].0 > 0)
        .collect();
    if kept.is_empty() {
        return BUILT_IN.to_vec();
    }
    // Most uses first, ties going to the lower hue.
    kept.sort_by_key(|&group| (Reverse(groups[group].0), group));
    kept.truncate(MOST_COLOURS);
    kept.sort_unstable();
    kept.into_iter()
        .map(|group| {
            groups[group]
                .1
                .expect("a used hue group has a colour")
                .colour
        })
        .collect()
}

fn is_grey_or_dark(colour: [u8; 3]) -> bool {
    let largest = colour.into_iter().max().unwrap_or(0);
    let smallest = colour.into_iter().min().unwrap_or(0);
    largest - smallest < GREY_SPREAD || largest < DARK_LARGEST
}

/// The hue group of `colour`, which is neither grey nor dark.
fn hue_group([r, g, b]: [u8; 3]) -> usize {
    let [r, g, b] = [r, g, b].map(f32::from);
    let largest = r.max(g).max(b);
    let spread = largest - r.min(g).min(b);
    let sixths = if largest == r {
        (g - b) / spread
    } else if largest == g {
        2.0 + (b - r) / spread
    } else {
        4.0 + (r - g) / spread
    };
    let hue = (sixths * 60.0).rem_euclid(360.0);
    // Rounding can bring a hue just under 360° up to 360°, which is 0°.
    (hue / 30.0) as usize % HUE_GROUPS
}
