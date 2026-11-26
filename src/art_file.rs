//! The art file: plain text with one line per row of cells, each line ending
//! in a reset (`ESC[0m`) and a newline, with no header.

use std::error::Error;
use std::fmt;
use std::fs;
use std::io;
use std::iter;
use std::path::{Path, PathBuf};

use unicode_width::UnicodeWidthChar;

const ESC: char = '\x1b';
const RESET: &str = "\x1b[0m";

/// The art read from an art file: its rows, as stored, and their common width.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Art {
    rows: Vec<String>,
    columns: usize,
}

impl Art {
    /// The rows, one per line of the file, each without its newline.
    pub fn rows(&self) -> &[String] {
        &self.rows
    }

    /// The display width of every row, counting only text outside escape
    /// sequences.
    pub fn columns(&self) -> usize {
        self.columns
    }
}

/// Why an art file could not be read.
#[derive(Debug)]
pub enum ArtFileError {
    /// The file could not be read as UTF-8 text.
    Io { path: PathBuf, source: io::Error },
    /// The file holds no rows.
    Empty { path: PathBuf },
    /// A row's width differs from the first row's.
    UnevenRows {
        path: PathBuf,
        /// The 1-based number of the first line whose width differs.
        line: usize,
        width: usize,
        first_width: usize,
    },
}

impl fmt::Display for ArtFileError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io { path, source } => {
                write!(f, "jaci: cannot read art file {}: {source}", path.display())
            }
            Self::Empty { path } => write!(f, "jaci: art file {} is empty", path.display()),
            Self::UnevenRows {
                path,
                line,
                width,
                first_width,
            } => write!(
                f,
                "jaci: art file {}: line {line} is {width} columns wide, line 1 is {first_width}",
                path.display()
            ),
        }
    }
}

impl Error for ArtFileError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Io { source, .. } => Some(source),
            Self::Empty { .. } | Self::UnevenRows { .. } => None,
        }
    }
}

/// Writes `rows` to `path` as an art file, appending a reset and a newline to
/// each row.
pub fn write(path: &Path, rows: &[impl AsRef<str>]) -> io::Result<()> {
    let mut content = String::new();
    for row in rows {
        content.push_str(row.as_ref());
        content.push_str(RESET);
        content.push('\n');
    }
    fs::write(path, content)
}

/// Reads the art file at `path`, refusing an empty file and rows of
/// different widths.
pub fn read(path: &Path) -> Result<Art, ArtFileError> {
    let content = fs::read_to_string(path).map_err(|source| ArtFileError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    let rows: Vec<String> = content.lines().map(str::to_owned).collect();
    let Some(first) = rows.first() else {
        return Err(ArtFileError::Empty {
            path: path.to_path_buf(),
        });
    };
    let columns = visible_width(first);
    for (index, row) in rows.iter().enumerate().skip(1) {
        let width = visible_width(row);
        if width != columns {
            return Err(ArtFileError::UnevenRows {
                path: path.to_path_buf(),
                line: index + 1,
                width,
                first_width: columns,
            });
        }
    }
    Ok(Art { rows, columns })
}

/// The display width of `text`, skipping its escape sequences.
pub(crate) fn visible_width(text: &str) -> usize {
    tokens(text)
        .map(|token| match token {
            Token::Escape(_) => 0,
            Token::Char(c) => c.width().unwrap_or(0),
        })
        .sum()
}

/// One piece of text as a terminal reads it.
pub(crate) enum Token<'a> {
    /// A CSI sequence (`ESC [` up to its final byte in `@`..=`~`) or any
    /// other two-character escape.
    Escape(&'a str),
    /// A character outside escape sequences.
    Char(char),
}

/// The tokens of `text`, in order.
pub(crate) fn tokens(text: &str) -> impl Iterator<Item = Token<'_>> {
    let mut rest = text;
    iter::from_fn(move || {
        let mut chars = rest.char_indices();
        let (_, first) = chars.next()?;
        if first != ESC {
            rest = &rest[first.len_utf8()..];
            return Some(Token::Char(first));
        }
        let end = match chars.next() {
            Some((_, '[')) => chars
                .find(|&(_, c)| ('@'..='~').contains(&c))
                .map_or(rest.len(), |(index, c)| index + c.len_utf8()),
            Some((index, c)) => index + c.len_utf8(),
            None => rest.len(),
        };
        let (escape, tail) = rest.split_at(end);
        rest = tail;
        Some(Token::Escape(escape))
    })
}
