//! The sized art: the art files `jaci banner` encodes from the saved image,
//! one per art width, in `<config folder>/sizes/`. The 16 shown most
//! recently are kept, and a record beside the folder, `sizes.encode`,
//! holds the `[encode]` settings they were written with.

use std::cmp::Reverse;
use std::error::Error;
use std::fmt;
use std::fs::{self, File};
use std::io;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use crate::art_file::{self, Art, ArtFileError};
use crate::atomic_file;
use crate::config::Encode;
use crate::named::Named;

/// The config folder's subfolder that holds the sized art.
const FOLDER: &str = "sizes";
/// The config folder's record of the settings the sized art was written
/// with.
const SETTINGS: &str = "sizes.encode";
/// How many sized arts are kept: those shown most recently.
const KEPT: usize = 16;
/// The extension of a sized art's file name; other files in the folder,
/// such as the temporary file of a sized art being written, are not one.
const EXTENSION: &str = "ansi";

/// Why the sized art could not be kept: the path that failed and the error.
#[derive(Debug)]
pub struct SizedArtError {
    pub path: PathBuf,
    pub source: io::Error,
}

impl fmt::Display for SizedArtError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "jaci: cannot keep the sized art: {}: {}",
            self.path.display(),
            self.source
        )
    }
}

impl Error for SizedArtError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        Some(&self.source)
    }
}

/// The folder of the sized art in the config folder `folder`.
pub(crate) fn folder(folder: &Path) -> PathBuf {
    folder.join(FOLDER)
}

/// Deletes the folder of the sized art `sizes`, if there is one.
pub(crate) fn delete_all(sizes: &Path) -> io::Result<()> {
    match fs::remove_dir_all(sizes) {
        Err(error) if error.kind() != io::ErrorKind::NotFound => Err(error),
        _ => Ok(()),
    }
}

/// The sized art `columns` wide in the config folder `folder`, counted as
/// shown now. When the settings of `encode` that shape the art differ from
/// those the kept sized art was written with, all of it is deleted first.
/// A missing one is first written to the path given to `write`, and
/// written once more when it is gone again before it is read, as when a
/// banner started at the same time deletes all of it. Only the [`KEPT`]
/// shown most recently are left.
///
/// # Errors
///
/// What `write` returns, [`ArtFileError`] when the art cannot be read, and
/// [`SizedArtError`] when the record or a sized art cannot be read,
/// written, marked shown or deleted.
pub fn show<E>(
    folder: &Path,
    encode: &Encode,
    columns: u32,
    mut write: impl FnMut(&Path) -> Result<(), E>,
) -> Result<Art, E>
where
    E: From<SizedArtError> + From<ArtFileError>,
{
    let sizes = self::folder(folder);
    let record = folder.join(SETTINGS);
    let settings = settings(encode);
    if fs::read_to_string(&record).ok().as_deref() != Some(settings.as_str()) {
        delete_all(&sizes).map_err(failed(&sizes))?;
        atomic_file::write(&record, settings.as_bytes()).map_err(failed(&record))?;
    }
    let path = sizes.join(format!("{columns}.{EXTENSION}"));
    let art = match art_file::read(&path) {
        Ok(art) => {
            File::options()
                .write(true)
                .open(&path)
                .and_then(|file| file.set_modified(SystemTime::now()))
                .map_err(failed(&path))?;
            art
        }
        Err(error) if error.is_missing() => {
            write(&path)?;
            match art_file::read(&path) {
                Err(error) if error.is_missing() => {
                    write(&path)?;
                    art_file::read(&path)?
                }
                read => read?,
            }
        }
        Err(error) => return Err(error.into()),
    };
    keep_recent(&sizes).map_err(failed(&sizes))?;
    Ok(art)
}

/// The settings of `encode` that shape a sized art, one `key = value` line
/// each: all that `jaci encode -w <columns>` reads from `[encode]`.
fn settings(encode: &Encode) -> String {
    let mut settings = format!(
        "mode = {}\nascii_style = {}\nratio = {}\n",
        toml::Value::from(encode.mode.name()),
        toml::Value::from(encode.ascii_style.name()),
        toml::Value::from(encode.ratio),
    );
    if let Some(charset) = &encode.charset {
        settings.push_str(&format!(
            "charset = {}\n",
            toml::Value::from(charset.to_string())
        ));
    }
    settings
}

/// Deletes all but the [`KEPT`] sized arts in `sizes` modified most
/// recently, leaving its other files. A sized art or a folder that a banner
/// started at the same time deletes first is skipped.
fn keep_recent(sizes: &Path) -> io::Result<()> {
    let entries = match fs::read_dir(sizes) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
        entries => entries?,
    };
    let mut files = Vec::new();
    for entry in entries {
        let entry = entry?;
        let path = entry.path();
        if path
            .extension()
            .is_none_or(|extension| extension != EXTENSION)
        {
            continue;
        }
        match entry.metadata().and_then(|metadata| metadata.modified()) {
            Ok(modified) => files.push((modified, path)),
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(error),
        }
    }
    if files.len() > KEPT {
        files.sort_unstable_by_key(|&(modified, _)| Reverse(modified));
        for (_, path) in &files[KEPT..] {
            match fs::remove_file(path) {
                Err(error) if error.kind() != io::ErrorKind::NotFound => return Err(error),
                _ => {}
            }
        }
    }
    Ok(())
}

/// Turns an error on `path` into a [`SizedArtError`].
fn failed(path: &Path) -> impl FnOnce(io::Error) -> SizedArtError + '_ {
    move |source| SizedArtError {
        path: path.to_path_buf(),
        source,
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use tempfile::TempDir;

    use super::*;

    /// Writes the file `name` in `sizes`, modified `seconds` after the epoch.
    fn add(sizes: &Path, name: &str, seconds: u64) {
        let path = sizes.join(name);
        fs::write(&path, "x\n").unwrap();
        File::options()
            .write(true)
            .open(&path)
            .unwrap()
            .set_modified(SystemTime::UNIX_EPOCH + Duration::from_secs(seconds))
            .unwrap();
    }

    #[test]
    fn only_sized_art_counts_toward_those_kept() {
        let sizes = TempDir::new().unwrap();
        // A temporary file another banner is writing, older than any art.
        add(sizes.path(), ".tmpAbC123", 0);
        for columns in 20..=36 {
            add(sizes.path(), &format!("{columns}.ansi"), columns);
        }

        keep_recent(sizes.path()).unwrap();

        assert!(sizes.path().join(".tmpAbC123").exists());
        for columns in 21..=36 {
            assert!(sizes.path().join(format!("{columns}.ansi")).exists());
        }
        assert!(!sizes.path().join("20.ansi").exists());
        assert_eq!(fs::read_dir(sizes.path()).unwrap().count(), 17);
    }

    #[test]
    fn folder_another_banner_deleted_has_nothing_to_trim() {
        let folder = TempDir::new().unwrap();

        keep_recent(&folder.path().join(FOLDER)).unwrap();
    }

    #[test]
    fn sized_art_deleted_once_written_is_written_again() {
        let folder = TempDir::new().unwrap();
        let mut writes = 0;

        let art = show::<Box<dyn Error>>(folder.path(), &Encode::default(), 20, |path| {
            writes += 1;
            atomic_file::write(path, b"ab\n")?;
            if writes == 1 {
                // Another banner drops the sized art as soon as it is written.
                delete_all(path.parent().unwrap())?;
            }
            Ok(())
        });

        assert_eq!(art.unwrap().rows(), ["ab"]);
        assert_eq!(writes, 2);
    }
}
