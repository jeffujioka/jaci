use std::fs;

use jaci::art_file::{self, ArtFileError};
use tempfile::TempDir;

#[test]
fn rows_end_in_reset() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("art.ansi");

    art_file::write(&path, &["\x1b[38;2;255;0;0m▀\x1b[0m", " "]).unwrap();

    assert_eq!(
        fs::read_to_string(&path).unwrap(),
        "\x1b[38;2;255;0;0m▀\x1b[0m\x1b[0m\n \x1b[0m\n"
    );
    let art = art_file::read(&path).unwrap();
    assert_eq!((art.columns(), art.rows().len()), (1, 2));
}

#[test]
fn size_from_visible_characters() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("art.ansi");
    fs::write(&path, "\x1b[38;2;1;2;3m▀▀▀▀▀\x1b[0m\n".repeat(3)).unwrap();

    let art = art_file::read(&path).unwrap();

    assert_eq!((art.columns(), art.rows().len()), (5, 3));
}

#[test]
fn empty_art_file_refused() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("empty.ansi");
    fs::write(&path, "").unwrap();

    let error = art_file::read(&path).unwrap_err();

    assert!(matches!(error, ArtFileError::Empty { .. }));
    assert_eq!(
        error.to_string(),
        format!("jaci: art file {} is empty", path.display())
    );
    assert!(error.to_string().ends_with("empty.ansi is empty"));
}

#[test]
fn uneven_rows_refused() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("uneven.ansi");
    fs::write(
        &path,
        "\x1b[38;2;1;2;3m▀▀▀▀▀\x1b[0m\n\x1b[38;2;1;2;3m▀▀▀▀\x1b[0m\n",
    )
    .unwrap();

    let error = art_file::read(&path).unwrap_err();

    assert!(matches!(error, ArtFileError::UnevenRows { .. }));
    assert_eq!(
        error.to_string(),
        format!(
            "jaci: art file {}: line 2 is 4 columns wide, line 1 is 5",
            path.display()
        )
    );
}
