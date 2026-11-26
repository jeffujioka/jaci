//! The system info cache: the file `<config folder>/cache` holding, as TOML
//! strings, the values of the static variables the config uses, so a banner
//! never runs the slow system tools that find them.

use std::collections::BTreeMap;
use std::error::Error;
use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::atomic_file;
use crate::config::{BlockText, Config};
use crate::named::Named;
use crate::sysinfo::StaticVariable;
use crate::template::{self, Segment};

/// The cache file's name in the config folder.
pub const FILE_NAME: &str = "cache";

/// Values of static variables, each saved or not.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Cache {
    /// Keyed by [`StaticVariable::name`], the keys of the file.
    values: BTreeMap<&'static str, String>,
}

impl Cache {
    /// The saved value of `variable`, `None` when the cache has none.
    pub fn get(&self, variable: StaticVariable) -> Option<&str> {
        self.values.get(variable.name()).map(String::as_str)
    }
}

impl FromIterator<(StaticVariable, String)> for Cache {
    fn from_iter<I: IntoIterator<Item = (StaticVariable, String)>>(values: I) -> Self {
        Self {
            values: values
                .into_iter()
                .map(|(variable, value)| (variable.name(), value))
                .collect(),
        }
    }
}

/// A cache file that cannot be read or written.
#[derive(Debug)]
pub enum CacheError {
    /// The file exists but could not be read.
    Read { path: PathBuf, source: io::Error },
    /// The file is not TOML of string values keyed by static variable names.
    Damaged { path: PathBuf },
    /// The file, or a folder it needs, could not be written.
    Write { path: PathBuf, source: io::Error },
}

impl fmt::Display for CacheError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Read { path, source } => {
                write!(f, "jaci: cannot read cache {}: {source}", path.display())
            }
            Self::Damaged { path } => write!(
                f,
                "jaci: cache {} is damaged; run 'jaci cache' to rebuild it",
                path.display()
            ),
            Self::Write { path, source } => {
                write!(f, "jaci: cannot write cache {}: {source}", path.display())
            }
        }
    }
}

impl Error for CacheError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Read { source, .. } | Self::Write { source, .. } => Some(source),
            Self::Damaged { .. } => None,
        }
    }
}

/// Reads the cache file at `path`, `None` when there is no file.
///
/// # Errors
///
/// [`CacheError::Read`] when the file exists but cannot be read, and
/// [`CacheError::Damaged`] when it is not UTF-8 TOML whose every key is a
/// static variable name with a string value.
pub fn read(path: &Path) -> Result<Option<Cache>, CacheError> {
    let input = match fs::read(path) {
        Ok(input) => input,
        Err(source) if source.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(source) => {
            return Err(CacheError::Read {
                path: path.to_path_buf(),
                source,
            });
        }
    };
    let damaged = || CacheError::Damaged {
        path: path.to_path_buf(),
    };
    let input = str::from_utf8(&input).map_err(|_| damaged())?;
    let table: BTreeMap<String, String> = toml::from_str(input).map_err(|_| damaged())?;
    let cache = table
        .into_iter()
        .map(|(name, value)| Some((StaticVariable::from_name(&name)?, value)))
        .collect::<Option<Cache>>()
        .ok_or_else(damaged)?;
    Ok(Some(cache))
}

/// Writes `cache` to the cache file at `path`, creating its missing folders
/// and replacing any file there in one step, so a reader sees the old file
/// or the new one, never part of it.
///
/// # Errors
///
/// [`CacheError::Write`] when a folder or the file cannot be written.
pub fn write(path: &Path, cache: &Cache) -> Result<(), CacheError> {
    toml::to_string(&cache.values)
        .map_err(io::Error::other)
        .and_then(|text| atomic_file::write(path, text.as_bytes()))
        .map_err(|source| CacheError::Write {
            path: path.to_path_buf(),
            source,
        })
}

/// The static variables that the config's text blocks use, in the order of
/// [`StaticVariable::ALL`]: those named by a placeholder in the literal text
/// of a block's `content`, never in a command.
pub fn used_variables(config: &Config) -> Vec<StaticVariable> {
    let named: Vec<StaticVariable> = config
        .banner
        .text
        .blocks
        .iter()
        .filter_map(|block| match &block.text {
            BlockText::Content(segments) => Some(segments),
            BlockText::Command(_) => None,
        })
        .flatten()
        .filter_map(|segment| match segment {
            Segment::Text(text) => Some(text),
            Segment::Command(_) => None,
        })
        .flat_map(|text| template::placeholders(text))
        .filter_map(|placeholder| StaticVariable::from_name(placeholder.name))
        .collect();
    StaticVariable::ALL
        .iter()
        .copied()
        .filter(|variable| named.contains(variable))
        .collect()
}

/// Resolves the static variables that `config` uses and writes exactly
/// those to the cache file at `path`, as [`write()`] does.
///
/// # Errors
///
/// [`CacheError::Write`] when a folder or the file cannot be written.
pub fn refresh(config: &Config, path: &Path) -> Result<(), CacheError> {
    let cache = used_variables(config)
        .into_iter()
        .map(|variable| (variable, variable.resolve()))
        .collect();
    write(path, &cache)
}
