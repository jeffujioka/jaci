use jaci::art_file;
use jaci::palette;
use tempfile::TempDir;

/// The palette of an art file of one row whose cells use `colours`, in order,
/// one cell each.
fn palette_of(colours: &[[u8; 3]]) -> Vec<[u8; 3]> {
    let row: String = colours
        .iter()
        .map(|[r, g, b]| format!("\x1b[38;2;{r};{g};{b}m▀"))
        .collect();
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("art.ansi");
    art_file::write(&path, &[row]).unwrap();
    palette::of(&art_file::read(&path).unwrap())
}

const TERRACOTTA: [u8; 3] = [0xCC, 0x6E, 0x4D];
const SAND: [u8; 3] = [0xE3, 0xCE, 0x7B];
const LIME: [u8; 3] = [0xC6, 0xDB, 0x7A];
const GREEN: [u8; 3] = [0x94, 0xCF, 0x77];
const SKY: [u8; 3] = [0x68, 0x9E, 0xC1];
const BLUE: [u8; 3] = [0x42, 0x7A, 0xB9];
const GREY: [u8; 3] = [0x80, 0x80, 0x80];

#[test]
fn colours_come_in_hue_order() {
    assert_eq!(palette_of(&[BLUE, TERRACOTTA]), [TERRACOTTA, BLUE]);
}

#[test]
fn grey_and_dark_colours_skipped() {
    assert_eq!(
        palette_of(&[GREY, [0x2A, 0x1A, 0x60], TERRACOTTA]),
        [TERRACOTTA]
    );
}

#[test]
fn one_colour_per_hue_group() {
    assert_eq!(
        palette_of(&[TERRACOTTA, TERRACOTTA, TERRACOTTA, [0xD0, 0x70, 0x4F]]),
        [TERRACOTTA]
    );
}

#[test]
fn at_most_six_colours() {
    let six = [TERRACOTTA, SAND, LIME, GREEN, SKY, BLUE];
    let mut colours: Vec<[u8; 3]> = six.iter().flat_map(|&colour| [colour, colour]).collect();
    colours.push([0xC1, 0x4D, 0x9E]);

    assert_eq!(palette_of(&colours), six);
}

#[test]
fn built_in_set_when_no_colour_left() {
    assert_eq!(
        palette_of(&[GREY]),
        [TERRACOTTA, SAND, LIME, GREEN, SKY, BLUE]
    );
}
