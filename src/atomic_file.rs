//! Files replaced in one step, so a reader sees the old file or the new one,
//! never part of either.

use std::fs;
use std::io::{self, Write as _};
use std::path::Path;

use tempfile::NamedTempFile;

/// Writes `contents` to a temporary file beside `path`, creating the missing
/// folders, then renames it to `path`, replacing any file there.
pub(crate) fn write(path: &Path, contents: &[u8]) -> io::Result<()> {
    let folder = path
        .parent()
        .filter(|folder| !folder.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    fs::create_dir_all(folder)?;
    let mut file = NamedTempFile::new_in(folder)?;
    file.write_all(contents)?;
    file.persist(path)?;
    Ok(())
}
