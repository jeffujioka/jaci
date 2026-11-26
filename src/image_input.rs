//! Image input: reads a local PNG, JPEG, WebP, BMP, GIF (its first frame) or
//! TIFF file into the RGBA pixels the renderers draw.

use std::error::Error;
use std::fmt;
use std::io;
use std::path::{Path, PathBuf};

use image::{ImageError, ImageReader, RgbaImage};

/// Why an image could not be read.
#[derive(Debug)]
pub enum ImageInputError {
    /// No file exists at the path.
    NotFound { path: PathBuf, source: io::Error },
    /// The file exists but cannot be read, such as a folder or a file
    /// without read permission.
    Read { path: PathBuf, source: io::Error },
    /// The file could not be decoded as a supported image.
    Decode { path: PathBuf, source: ImageError },
}

impl fmt::Display for ImageInputError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotFound { path, .. } => {
                write!(f, "jaci: image not found: {}", path.display())
            }
            Self::Read { path, source } => {
                write!(f, "jaci: cannot read image {}: {source}", path.display())
            }
            Self::Decode { path, .. } => write!(
                f,
                "jaci: cannot decode image {}: unsupported or corrupt file",
                path.display()
            ),
        }
    }
}

impl Error for ImageInputError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::NotFound { source, .. } | Self::Read { source, .. } => Some(source),
            Self::Decode { source, .. } => Some(source),
        }
    }
}

/// Reads the image at `path`, telling its format from the file's contents, and
/// returns its pixels as RGBA; an animated GIF gives its first frame.
///
/// # Errors
///
/// [`ImageInputError::NotFound`] when no file exists at `path`,
/// [`ImageInputError::Read`] when the file cannot be opened or its start
/// read, and [`ImageInputError::Decode`] for any decode failure.
pub fn read(path: impl AsRef<Path>) -> Result<RgbaImage, ImageInputError> {
    let path = path.as_ref();
    let read_error = |source: io::Error| match source.kind() {
        io::ErrorKind::NotFound => ImageInputError::NotFound {
            path: path.to_path_buf(),
            source,
        },
        _ => ImageInputError::Read {
            path: path.to_path_buf(),
            source,
        },
    };
    let image = ImageReader::open(path)
        .map_err(read_error)?
        .with_guessed_format()
        .map_err(read_error)?
        .decode()
        .map_err(|source| ImageInputError::Decode {
            path: path.to_path_buf(),
            source,
        })?;
    // Moves the buffer when the decode is already RGBA8; converts otherwise.
    Ok(image.into_rgba8())
}
