//! Files replaced in one step, so a reader sees the old file or the new one,
//! never part of either.

use std::fs;
use std::io::{self, Write as _};
use std::path::{Path, PathBuf};

use tempfile::NamedTempFile;

/// The most symlinks [`final_target`] follows, as Linux's `MAXSYMLINKS`.
const MAX_LINKS: usize = 40;

/// Writes `contents` to a temporary file beside the file `path` names, then
/// renames it over that file, replacing any file there and creating the
/// missing folders. When `path` is a symlink, the file it names is its final
/// target, even a missing one, so the link stays in place.
///
/// # Errors
///
/// Any error of reading the links, creating the folders or writing and
/// renaming the file; `ELOOP` past [`MAX_LINKS`] links.
pub(crate) fn write(path: &Path, contents: &[u8]) -> io::Result<()> {
    let path = final_target(path)?;
    let folder = path
        .parent()
        .filter(|folder| !folder.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    fs::create_dir_all(folder)?;
    let mut file = NamedTempFile::new_in(folder)?;
    file.write_all(contents)?;
    file.persist(&path)?;
    Ok(())
}

/// `path` with every symlink it ends in followed, a relative target taken
/// from the link's folder; the path of a missing target as it is.
fn final_target(path: &Path) -> io::Result<PathBuf> {
    let mut path = path.to_owned();
    for _ in 0..MAX_LINKS {
        match fs::read_link(&path) {
            // Joining an absolute target replaces the folder.
            Ok(target) => path = path.parent().unwrap_or(Path::new("")).join(target),
            // Not a symlink, or nothing there.
            Err(error)
                if matches!(
                    error.kind(),
                    io::ErrorKind::InvalidInput | io::ErrorKind::NotFound
                ) =>
            {
                return Ok(path);
            }
            Err(error) => return Err(error),
        }
    }
    Err(io::Error::from_raw_os_error(libc::ELOOP))
}

#[cfg(test)]
mod tests {
    use std::os::unix::fs::symlink;

    use tempfile::TempDir;

    use super::*;

    #[test]
    fn writes_through_a_chain_of_relative_links() {
        let folder = TempDir::new().unwrap();
        let target = folder.path().join("real/file");
        fs::create_dir_all(target.parent().unwrap()).unwrap();
        fs::write(&target, "old").unwrap();
        symlink("real/file", folder.path().join("middle")).unwrap();
        let link = folder.path().join("link");
        symlink("middle", &link).unwrap();

        write(&link, b"new").unwrap();

        assert!(fs::symlink_metadata(&link).unwrap().is_symlink());
        assert_eq!(fs::read_link(&link).unwrap(), Path::new("middle"));
        assert_eq!(fs::read_to_string(&target).unwrap(), "new");
    }

    #[test]
    fn dangling_link_creates_its_target() {
        let folder = TempDir::new().unwrap();
        let target = folder.path().join("missing/file");
        let link = folder.path().join("link");
        symlink(&target, &link).unwrap();

        write(&link, b"new").unwrap();

        assert!(fs::symlink_metadata(&link).unwrap().is_symlink());
        assert_eq!(fs::read_to_string(&target).unwrap(), "new");
    }

    #[test]
    fn link_loop_is_an_error() {
        let folder = TempDir::new().unwrap();
        let link = folder.path().join("link");
        symlink("link", &link).unwrap();

        assert!(write(&link, b"new").is_err());
        assert!(fs::symlink_metadata(&link).unwrap().is_symlink());
    }
}
