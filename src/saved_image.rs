//! The saved image: the image `jaci init` and `jaci encode` read, kept in the
//! config folder as a PNG no wider than the banner's widest art needs, and
//! the sized art encoded from it, which a new saved image makes stale.

use std::borrow::Cow;
use std::error::Error;
use std::fmt;
use std::io::{self, Cursor};
use std::path::{Path, PathBuf};

use image::{ImageError, ImageFormat, RgbaImage};

use crate::atomic_file;
use crate::image_input::ImageInputError;
use crate::size;
use crate::sized_art;

/// The saved image's name in the config folder.
pub const FILE_NAME: &str = "image.png";

/// Why the saved image could not be written or the sized art deleted: the
/// path that failed and the error.
#[derive(Debug)]
pub struct SaveError {
    pub path: PathBuf,
    pub source: io::Error,
}

impl fmt::Display for SaveError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "jaci: cannot save the image: {}: {}",
            self.path.display(),
            self.source
        )
    }
}

impl Error for SaveError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        Some(&self.source)
    }
}

/// Writes `image` as the saved image in `folder`, resampled to 2 ×
/// `max_width` pixels wide, its height in proportion, when wider than that,
/// else as it is, then deletes every sized art in `folder`.
///
/// # Errors
///
/// [`SaveError`] naming the saved image when it cannot be encoded or
/// written, or the sized art's folder when it cannot be deleted.
pub fn write(folder: &Path, image: &RgbaImage, max_width: u32) -> Result<(), SaveError> {
    let widest = max_width.saturating_mul(2);
    let (width, height) = image.dimensions();
    let image = if width > widest {
        let shrunk = size::rounded(f64::from(height) * f64::from(widest) / f64::from(width));
        Cow::Owned(size::resample(image, widest, shrunk))
    } else {
        Cow::Borrowed(image)
    };
    let path = folder.join(FILE_NAME);
    let mut png = Vec::new();
    image
        .write_to(&mut Cursor::new(&mut png), ImageFormat::Png)
        .map_err(io::Error::other)
        .and_then(|()| atomic_file::write(&path, &png))
        .map_err(|source| SaveError { path, source })?;
    let sizes = sized_art::folder(folder);
    sized_art::delete_all(&sizes).map_err(|source| SaveError {
        path: sizes,
        source,
    })
}

/// The width and height in pixels of the saved image in `folder`, read
/// from its header, or `None` when there is none.
///
/// # Errors
///
/// [`ImageInputError::Read`] when the saved image cannot be read and
/// [`ImageInputError::Decode`] when its header cannot be decoded.
pub fn dimensions(folder: &Path) -> Result<Option<(u32, u32)>, ImageInputError> {
    let path = folder.join(FILE_NAME);
    match image::image_dimensions(&path) {
        Ok(dimensions) => Ok(Some(dimensions)),
        Err(ImageError::IoError(source)) if source.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(ImageError::IoError(source)) => Err(ImageInputError::Read { path, source }),
        Err(source) => Err(ImageInputError::Decode { path, source }),
    }
}
