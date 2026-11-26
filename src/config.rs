//! The config folder and its `config.toml`: every key optional, each one
//! checked against its allowed values when the file is loaded.

use std::error::Error;
use std::ffi::OsStr;
use std::fmt;
use std::fs;
use std::io;
use std::ops::Range;
use std::path::{Path, PathBuf};

use toml::Spanned;
use toml::de::{DeString, DeTable, DeValue};

use crate::render::{Charset, Mode};
use crate::size::{self, DEFAULT_COLUMNS, DEFAULT_RATIO};
use crate::template::{self, Segment};

const FOLDER_NAME: &str = "jaci";
/// The config file's name in the config folder.
pub const FILE_NAME: &str = "config.toml";
/// The default art file's name in the config folder.
pub const DEFAULT_IMAGE: &str = "art.ansi";
pub(crate) const DEFAULT_PADDING: u32 = 4;
/// The start of a `[banner].image` that names the home folder.
pub(crate) const HOME_PREFIX: &str = "~/";

/// The config folder: `<xdg_config_home>/jaci` when `XDG_CONFIG_HOME` is set
/// and not empty, else `<home>/.config/jaci`.
pub fn folder(xdg_config_home: Option<&OsStr>, home: &Path) -> PathBuf {
    match xdg_config_home {
        Some(xdg_config_home) if !xdg_config_home.is_empty() => {
            Path::new(xdg_config_home).join(FOLDER_NAME)
        }
        _ => home.join(".config").join(FOLDER_NAME),
    }
}

/// The settings of `config.toml`, each one its default when the file leaves
/// it out.
#[derive(Debug, Clone, PartialEq)]
pub struct Config {
    pub encode: Encode,
    pub banner: Banner,
}

/// `[encode]`.
#[derive(Debug, Clone, PartialEq)]
pub struct Encode {
    pub mode: Mode,
    /// At least 1.
    pub width: u32,
    /// Finite and above 0.
    pub ratio: f64,
    pub charset: Charset,
}

impl Default for Encode {
    fn default() -> Self {
        Self {
            mode: Mode::Halfblock,
            width: DEFAULT_COLUMNS,
            ratio: DEFAULT_RATIO,
            charset: Charset::default(),
        }
    }
}

/// `[banner]`.
#[derive(Debug, Clone, PartialEq)]
pub struct Banner {
    /// The art file, with a leading `~/` already the home folder.
    pub image: PathBuf,
    /// The columns between the art and the text panel.
    pub padding: u32,
    pub text: Text,
}

/// `[banner.text]`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Text {
    pub align: Align,
    pub valign: Valign,
    /// In the order of the file.
    pub blocks: Vec<Block>,
}

/// `banner.text.align`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Align {
    #[default]
    Left,
    Center,
    Right,
}

impl Align {
    /// Every alignment, in the order the messages list them.
    pub const ALL: [Self; 3] = [Self::Left, Self::Center, Self::Right];

    /// The name the config gives the alignment.
    pub const fn name(self) -> &'static str {
        match self {
            Self::Left => "left",
            Self::Center => "center",
            Self::Right => "right",
        }
    }
}

/// `banner.text.valign`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Valign {
    #[default]
    Top,
    Center,
    Bottom,
}

impl Valign {
    /// Every vertical alignment, in the order the messages list them.
    pub const ALL: [Self; 3] = [Self::Top, Self::Center, Self::Bottom];

    /// The name the config gives the vertical alignment.
    pub const fn name(self) -> &'static str {
        match self {
            Self::Top => "top",
            Self::Center => "center",
            Self::Bottom => "bottom",
        }
    }
}

/// One `[[banner.text.blocks]]` entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Block {
    pub text: BlockText,
    pub style: Style,
    /// `R`, `G` and `B`; `None` keeps the terminal's colour.
    pub color: Option<[u8; 3]>,
}

/// Where a block's text comes from: exactly one of `content` and `command`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BlockText {
    /// `content`, split into literal text and inline commands.
    Content(Vec<Segment>),
    /// `command`, never empty.
    Command(String),
}

/// `banner.text.blocks[n].style`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Style {
    #[default]
    Normal,
    Bold,
    Dim,
}

impl Style {
    /// Every style, in the order the messages list them.
    pub const ALL: [Self; 3] = [Self::Normal, Self::Bold, Self::Dim];

    /// The name the config gives the style.
    pub const fn name(self) -> &'static str {
        match self {
            Self::Normal => "normal",
            Self::Bold => "bold",
            Self::Dim => "dim",
        }
    }
}

/// Why the config could not be loaded. Every message reads
/// `jaci: config <path>: line <n>: <problem>`, without `line <n>: ` when the
/// line is unknown.
#[derive(Debug)]
pub enum ConfigError {
    /// The file exists but could not be read as UTF-8 text.
    Read { path: PathBuf, source: io::Error },
    /// The file is not valid TOML.
    Syntax {
        path: PathBuf,
        /// The 1-based line of the error, when the parser locates it.
        line: Option<usize>,
        /// Boxed, as the parser's error is large.
        source: Box<toml::de::Error>,
    },
    /// A key is unknown, or its value is not one the key allows.
    Invalid {
        path: PathBuf,
        /// The 1-based line of the key or value at fault.
        line: usize,
        problem: String,
    },
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Read { path, source } => {
                write!(f, "jaci: config {}: cannot read: {source}", path.display())
            }
            Self::Syntax {
                path,
                line: Some(line),
                source,
            } => write!(
                f,
                "jaci: config {}: line {line}: {}",
                path.display(),
                source.message()
            ),
            Self::Syntax {
                path,
                line: None,
                source,
            } => write!(f, "jaci: config {}: {}", path.display(), source.message()),
            Self::Invalid {
                path,
                line,
                problem,
            } => write!(f, "jaci: config {}: line {line}: {problem}", path.display()),
        }
    }
}

impl Error for ConfigError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Read { source, .. } => Some(source),
            Self::Syntax { source, .. } => Some(source),
            Self::Invalid { .. } => None,
        }
    }
}

/// Loads `<folder>/config.toml`, every key its default when the file is
/// missing or leaves the key out. The default image is `<folder>/art.ansi`,
/// and a leading `~/` in `[banner].image` is `home`.
///
/// # Errors
///
/// [`ConfigError::Read`] when the file exists but cannot be read,
/// [`ConfigError::Syntax`] when it is not TOML and [`ConfigError::Invalid`]
/// for an unknown key, a value the key does not allow, a text block without
/// exactly one of `content` and `command`, or an unclosed `{cmd:`.
pub fn load(folder: &Path, home: &Path) -> Result<Config, ConfigError> {
    let path = folder.join(FILE_NAME);
    let mut config = Config {
        encode: Encode::default(),
        banner: Banner {
            image: folder.join(DEFAULT_IMAGE),
            padding: DEFAULT_PADDING,
            text: Text::default(),
        },
    };
    let input = match fs::read_to_string(&path) {
        Ok(input) => input,
        Err(source) if source.kind() == io::ErrorKind::NotFound => return Ok(config),
        Err(source) => return Err(ConfigError::Read { path, source }),
    };
    let document = match DeTable::parse(&input) {
        Ok(document) => document,
        Err(source) => {
            let line = source.span().map(|span| line_of(&input, span.start));
            return Err(ConfigError::Syntax {
                path,
                line,
                source: Box::new(source),
            });
        }
    };
    match read_root(document.get_ref(), home, &mut config) {
        Ok(()) => Ok(config),
        Err(Invalid { offset, problem }) => Err(ConfigError::Invalid {
            line: line_of(&input, offset),
            path,
            problem,
        }),
    }
}

/// A problem found in the parsed document, at a byte offset of the input.
struct Invalid {
    offset: usize,
    problem: String,
}

impl Invalid {
    fn at(span: Range<usize>, problem: String) -> Self {
        Self {
            offset: span.start,
            problem,
        }
    }
}

type Key<'a> = Spanned<DeString<'a>>;
type Value<'a> = Spanned<DeValue<'a>>;

/// The 1-based line holding byte `offset` of `input`.
fn line_of(input: &str, offset: usize) -> usize {
    input.as_bytes()[..offset.min(input.len())]
        .iter()
        .filter(|&&byte| byte == b'\n')
        .count()
        + 1
}

fn read_root(root: &DeTable<'_>, home: &Path, config: &mut Config) -> Result<(), Invalid> {
    for (key, value) in entries(root) {
        match key.get_ref().as_ref() {
            "encode" => read_encode(table_of(value, "encode")?, &mut config.encode)?,
            "banner" => read_banner(table_of(value, "banner")?, home, &mut config.banner)?,
            _ => return Err(unknown_key(key, "")),
        }
    }
    Ok(())
}

fn read_encode(table: &DeTable<'_>, encode: &mut Encode) -> Result<(), Invalid> {
    for (key, value) in entries(table) {
        match key.get_ref().as_ref() {
            "mode" => {
                encode.mode = one_of(
                    value,
                    "encode.mode",
                    &Mode::ALL.map(|mode| (mode.name(), mode)),
                )?;
            }
            "width" => encode.width = whole_number(value, "encode.width", 1)?,
            "ratio" => encode.ratio = number_above_zero(value, "encode.ratio")?,
            "charset" => {
                const VALUES: &str = "text of at least one character";
                encode.charset = Charset::new(text(value, "encode.charset", VALUES)?)
                    .ok_or_else(|| must_be(value, "encode.charset", VALUES))?;
            }
            _ => return Err(unknown_key(key, "encode")),
        }
    }
    Ok(())
}

fn read_banner(table: &DeTable<'_>, home: &Path, banner: &mut Banner) -> Result<(), Invalid> {
    for (key, value) in entries(table) {
        match key.get_ref().as_ref() {
            "image" => {
                const VALUES: &str = "a file path; a leading ~/ is the home folder";
                let image = non_empty_text(value, "banner.image", VALUES)?;
                banner.image = match image.strip_prefix(HOME_PREFIX) {
                    Some(relative) => home.join(relative),
                    None => PathBuf::from(image),
                };
            }
            "padding" => banner.padding = whole_number(value, "banner.padding", 0)?,
            "text" => read_text(table_of(value, "banner.text")?, &mut banner.text)?,
            _ => return Err(unknown_key(key, "banner")),
        }
    }
    Ok(())
}

fn read_text(table: &DeTable<'_>, text: &mut Text) -> Result<(), Invalid> {
    for (key, value) in entries(table) {
        match key.get_ref().as_ref() {
            "align" => {
                text.align = one_of(
                    value,
                    "banner.text.align",
                    &Align::ALL.map(|align| (align.name(), align)),
                )?;
            }
            "valign" => {
                text.valign = one_of(
                    value,
                    "banner.text.valign",
                    &Valign::ALL.map(|valign| (valign.name(), valign)),
                )?;
            }
            "blocks" => {
                let DeValue::Array(blocks) = value.get_ref() else {
                    return Err(must_be(value, "banner.text.blocks", "an array of tables"));
                };
                text.blocks = blocks
                    .iter()
                    .enumerate()
                    .map(|(index, block)| read_block(block, index + 1))
                    .collect::<Result<_, _>>()?;
            }
            _ => return Err(unknown_key(key, "banner.text")),
        }
    }
    Ok(())
}

/// Reads block `number`, counted from 1.
fn read_block(value: &Value<'_>, number: usize) -> Result<Block, Invalid> {
    let scope = format!("banner.text.blocks[{number}]");
    let table = table_of(value, &scope)?;
    let mut content = None;
    let mut command = None;
    let mut style = Style::default();
    let mut color = None;
    for (key, value) in entries(table) {
        let name = format!("{scope}.{}", key.get_ref());
        match key.get_ref().as_ref() {
            "content" => {
                let text = text(value, &name, "text with variables and {cmd:...}")?;
                let segments = template::parse(text).map_err(|unclosed| {
                    Invalid::at(value.span(), format!("{name} has an {unclosed}"))
                })?;
                content = Some(BlockText::Content(segments));
            }
            "command" => {
                let text = non_empty_text(value, &name, "a shell command")?;
                command = Some(BlockText::Command(text.to_owned()));
            }
            "style" => {
                style = one_of(value, &name, &Style::ALL.map(|style| (style.name(), style)))?;
            }
            "color" => color = Some(colour(value, &name)?),
            _ => return Err(unknown_key(key, &scope)),
        }
    }
    let text = match (content, command) {
        (Some(text), None) | (None, Some(text)) => text,
        _ => {
            return Err(Invalid::at(
                value.span(),
                format!("{scope} needs exactly one of content and command"),
            ));
        }
    };
    Ok(Block { text, style, color })
}

/// The entries of `table` in the order of the file.
fn entries<'t, 'i>(table: &'t DeTable<'i>) -> Vec<(&'t Key<'i>, &'t Value<'i>)> {
    let mut entries: Vec<_> = table.iter().collect();
    entries.sort_by_key(|(key, _)| key.span().start);
    entries
}

fn table_of<'t, 'i>(value: &'t Value<'i>, name: &str) -> Result<&'t DeTable<'i>, Invalid> {
    value
        .get_ref()
        .as_table()
        .ok_or_else(|| must_be(value, name, "a table"))
}

fn unknown_key(key: &Key<'_>, scope: &str) -> Invalid {
    let key_name = key.get_ref();
    let name = if scope.is_empty() {
        key_name.to_string()
    } else {
        format!("{scope}.{key_name}")
    };
    Invalid::at(key.span(), format!("unknown key {name}"))
}

/// `<name> must be <values>`, at `value`.
fn must_be(value: &Value<'_>, name: &str, values: &str) -> Invalid {
    Invalid::at(value.span(), format!("{name} must be {values}"))
}

/// The value matching one of `choices`, which also name the allowed values.
fn one_of<T: Copy>(value: &Value<'_>, name: &str, choices: &[(&str, T)]) -> Result<T, Invalid> {
    value
        .get_ref()
        .as_str()
        .and_then(|text| {
            choices
                .iter()
                .find(|(choice, _)| *choice == text)
                .map(|&(_, choice)| choice)
        })
        .ok_or_else(|| {
            let names: Vec<_> = choices.iter().map(|(choice, _)| *choice).collect();
            must_be(value, name, &format!("one of {}", names.join(", ")))
        })
}

/// A whole number of at least `min` that fits in a `u32`.
fn whole_number(value: &Value<'_>, name: &str, min: u32) -> Result<u32, Invalid> {
    value
        .get_ref()
        .as_integer()
        .and_then(|integer| u32::from_str_radix(integer.as_str(), integer.radix()).ok())
        .filter(|&number| number >= min)
        .ok_or_else(|| must_be(value, name, &format!("a whole number of at least {min}")))
}

/// A finite number above 0, written as an integer or a float.
fn number_above_zero(value: &Value<'_>, name: &str) -> Result<f64, Invalid> {
    let number = match value.get_ref() {
        DeValue::Integer(integer) => i64::from_str_radix(integer.as_str(), integer.radix())
            .ok()
            .map(|integer| integer as f64),
        DeValue::Float(float) => float.as_str().parse::<f64>().ok(),
        _ => None,
    };
    number
        .filter(|&number| size::valid_ratio(number))
        .ok_or_else(|| must_be(value, name, "a number above 0"))
}

fn text<'t>(value: &'t Value<'_>, name: &str, values: &str) -> Result<&'t str, Invalid> {
    value
        .get_ref()
        .as_str()
        .ok_or_else(|| must_be(value, name, values))
}

fn non_empty_text<'t>(value: &'t Value<'_>, name: &str, values: &str) -> Result<&'t str, Invalid> {
    let text = text(value, name, values)?;
    if text.is_empty() {
        Err(must_be(value, name, values))
    } else {
        Ok(text)
    }
}

/// A colour written as `#RRGGBB` in hex digits of either case.
fn colour(value: &Value<'_>, name: &str) -> Result<[u8; 3], Invalid> {
    let invalid = || must_be(value, name, "a colour as #RRGGBB");
    let hex = text(value, name, "a colour as #RRGGBB")?
        .strip_prefix('#')
        .filter(|hex| hex.len() == 6 && hex.bytes().all(|byte| byte.is_ascii_hexdigit()))
        .ok_or_else(invalid)?;
    let channel = |at: usize| u8::from_str_radix(&hex[at..at + 2], 16).map_err(|_| invalid());
    Ok([channel(0)?, channel(2)?, channel(4)?])
}
