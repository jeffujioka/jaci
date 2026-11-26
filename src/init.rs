//! `jaci init`'s config templates, the full one and the minimal one, and the
//! questions it asks on stdin and stdout.

use std::error::Error;
use std::fmt;
use std::io::{self, BufRead, Write};
use std::path::{Path, PathBuf};

use crate::config::{self, Align, DEFAULT_PADDING, Encode, HOME_PREFIX, Style, Valign};
use crate::named::Named;
use crate::render::{DEFAULT_CHARSET, Mode};
use crate::sysinfo::{DynamicVariable, StaticVariable};

/// One text block per built-in variable.
pub const BLOCK_COUNT: usize = StaticVariable::ALL.len() + DynamicVariable::ALL.len();
/// Each block's label, in the order of [`variable_names`].
const LABELS: [&str; BLOCK_COUNT] = ["Host", "OS", "CPU", "GPU", "RAM", "Disk", "Uptime"];

/// The built-in variables' names: the static ones, then the dynamic ones.
fn variable_names() -> impl Iterator<Item = &'static str> {
    let statics = StaticVariable::ALL.iter().map(|variable| variable.name());
    let dynamics = DynamicVariable::ALL.iter().map(|variable| variable.name());
    statics.chain(dynamics)
}

/// The text of each built-in variable's block, such as `Host: {hostname}`.
fn block_contents() -> impl Iterator<Item = String> {
    LABELS
        .into_iter()
        .zip(variable_names())
        .map(|(label, name)| format!("{label}: {{{name}}}"))
}

/// What the full config sets that init can ask for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Settings {
    pub mode: Mode,
    pub width: u32,
    /// Whether each built-in variable's block is written, in the order of
    /// the variables.
    pub shown: [bool; BLOCK_COUNT],
}

impl Default for Settings {
    /// The config defaults, every block shown.
    fn default() -> Self {
        let encode = Encode::default();
        Self {
            mode: encode.mode,
            width: encode.width,
            shown: [true; BLOCK_COUNT],
        }
    }
}

impl Settings {
    /// The `[encode]` keys a config written with these settings holds: their
    /// mode and width, every other key its default.
    pub fn encode(&self) -> Encode {
        Encode {
            mode: self.mode,
            width: self.width,
            ..Encode::default()
        }
    }
}

/// Why init stopped.
#[derive(Debug)]
pub enum InitError {
    /// Neither the command line nor the answer names an image.
    NoImage,
    /// The overwrite of the config file at `path` was declined.
    Cancelled { path: PathBuf },
    /// The render mode answer is not a mode's name.
    InvalidMode(String),
    /// The width answer is not a whole number of at least 1.
    InvalidWidth(String),
    /// A yes or no answer is neither.
    InvalidAnswer(String),
    /// A question could not be asked, or its answer read.
    Prompt(io::Error),
    /// The art file's path cannot be written as TOML text.
    NotUtf8 { path: PathBuf },
    /// The config file could not be written.
    Write { path: PathBuf, source: io::Error },
}

impl fmt::Display for InitError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoImage => {
                f.write_str("Which image should jaci use? Run 'jaci init <image>' to get started.")
            }
            Self::Cancelled { path } => {
                write!(f, "jaci: init cancelled; {} left unchanged", path.display())
            }
            Self::InvalidMode(answer) => write!(f, "jaci: '{answer}' is not a valid render mode"),
            Self::InvalidWidth(answer) => write!(f, "jaci: '{answer}' is not a valid width"),
            Self::InvalidAnswer(answer) => write!(f, "jaci: '{answer}' is not a valid answer"),
            Self::Prompt(source) => write!(f, "jaci: cannot ask init's questions: {source}"),
            Self::NotUtf8 { path } => write!(
                f,
                "jaci: cannot name the art file {} in the config: it is not UTF-8",
                path.display()
            ),
            Self::Write { path, source } => {
                write!(f, "jaci: cannot write config {}: {source}", path.display())
            }
        }
    }
}

impl Error for InitError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Prompt(source) | Self::Write { source, .. } => Some(source),
            Self::NoImage
            | Self::Cancelled { .. }
            | Self::InvalidMode(_)
            | Self::InvalidWidth(_)
            | Self::InvalidAnswer(_)
            | Self::NotUtf8 { .. } => None,
        }
    }
}

/// `[banner].image` for the art file at `art`: `~/` and the rest of the path
/// when it lies under `home`, so one config serves several machines, else
/// the path itself.
///
/// # Errors
///
/// [`InitError::NotUtf8`] when that text is not UTF-8.
pub fn image_value(art: &Path, home: &Path) -> Result<String, InitError> {
    let text = match art.strip_prefix(home) {
        Ok(relative) => relative
            .to_str()
            .map(|relative| format!("{HOME_PREFIX}{relative}")),
        Err(_) => art.to_str().map(str::to_owned),
    };
    text.ok_or_else(|| InitError::NotUtf8 {
        path: art.to_path_buf(),
    })
}

/// `value` written as a TOML value.
fn toml_value(value: impl Into<toml::Value>) -> String {
    value.into().to_string()
}

/// The full config: every key with its value and a comment line above it,
/// `[banner].image` set to `image`, the mode and width of `settings`, a
/// block for each variable `settings` shows, and commented-out examples of
/// the other block keys.
pub fn full(image: &str, settings: &Settings) -> String {
    let encode = settings.encode();
    let mut blocks = String::new();
    let shown = block_contents()
        .zip(settings.shown)
        .filter_map(|(content, shown)| shown.then_some(content));
    for (index, content) in shown.enumerate() {
        blocks.push_str("[[banner.text.blocks]]\n");
        if index == 0 {
            blocks.push_str(
                "# The text, with variables such as {hostname} and {cmd:...} for a\n\
                 # command's output.\n",
            );
        }
        blocks.push_str(&format!("content = {}\n", toml_value(content)));
        if index == 0 {
            blocks.push_str(&format!(
                "# The style: {}.\nstyle = {}\n",
                Style::one_of(),
                toml_value(Style::default().name())
            ));
        }
        blocks.push('\n');
    }
    format!(
        "\
# jaci's config. Every key is optional; left out, it takes its default.

[encode]
# The render mode: {modes}.
mode = {mode}
# The art's width in columns: a whole number of at least 1.
width = {width}
# Character width ÷ height, which keeps the art's proportions: a number above 0.
ratio = {ratio}
# The characters ascii mode draws with, darkest first.
charset = {charset}

[banner]
# The art file the banner shows; a leading ~/ is the home folder.
image = {image}
# The columns between the art and the text panel: a whole number of at least 0.
padding = {padding}

[banner.text]
# How the lines of the text panel line up: {aligns}.
align = {align}
# Where the text panel sits beside the art: {valigns}.
valign = {valign}

# The text panel: one block per line, in order. Each block holds exactly one of
# content and command.
{blocks}\
# A block can show a command's output inside its text:
# [[banner.text.blocks]]
# content = {cmd_example}
#
# or as the whole block, run through sh -c with a 3-second limit:
# [[banner.text.blocks]]
# command = {command_example}
# The colour as #RRGGBB; left out, the text keeps the terminal's colour.
# color = {color_example}
",
        modes = Mode::one_of(),
        mode = toml_value(encode.mode.name()),
        width = toml_value(i64::from(encode.width)),
        ratio = toml_value(encode.ratio),
        charset = toml_value(DEFAULT_CHARSET),
        image = toml_value(image),
        padding = toml_value(i64::from(DEFAULT_PADDING)),
        aligns = Align::one_of(),
        align = toml_value(Align::default().name()),
        valigns = Valign::one_of(),
        valign = toml_value(Valign::default().name()),
        cmd_example = toml_value("Shell: {cmd:basename \"$SHELL\"}"),
        command_example = toml_value("date '+%A, %d %B'"),
        color_example = toml_value("#87AFFF"),
    )
}

/// The minimal config: `[banner].image` set to `image` and the
/// `{hostname}` block.
pub fn minimal(image: &str) -> String {
    let host = block_contents()
        .next()
        .expect("the first built-in variable is the host name");
    format!(
        "[banner]\nimage = {}\n\n[[banner.text.blocks]]\ncontent = {}\n",
        toml_value(image),
        toml_value(host)
    )
}

/// Asks `<path> already exists. Overwrite it? [y/N] `.
///
/// # Errors
///
/// [`InitError::Cancelled`] unless the answer is `y` or `yes`, in any case,
/// and [`InitError::Prompt`] when the question cannot be asked.
pub fn confirm_overwrite(
    input: &mut impl BufRead,
    output: &mut impl Write,
    path: &Path,
) -> Result<(), InitError> {
    let question = format!("{} already exists. Overwrite it? [y/N] ", path.display());
    let answer = ask(input, output, &question)?;
    if is_yes(&answer) {
        Ok(())
    } else {
        Err(InitError::Cancelled {
            path: path.to_path_buf(),
        })
    }
}

/// Asks `Image path: `; a leading `~/` in the answer is the folder `home`.
///
/// # Errors
///
/// [`InitError::NoImage`] for an empty answer and [`InitError::Prompt`] when
/// the question cannot be asked.
pub fn ask_image(
    input: &mut impl BufRead,
    output: &mut impl Write,
    home: &Path,
) -> Result<PathBuf, InitError> {
    let answer = ask(input, output, "Image path: ")?;
    if answer.is_empty() {
        Err(InitError::NoImage)
    } else {
        Ok(config::expand_home(&answer, home))
    }
}

/// Asks for the render mode, the width, then whether to show each built-in
/// variable, an empty answer taking the default.
///
/// # Errors
///
/// [`InitError::InvalidMode`], [`InitError::InvalidWidth`] or
/// [`InitError::InvalidAnswer`] at the first answer that is not valid, and
/// [`InitError::Prompt`] when a question cannot be asked.
pub fn ask_settings(
    input: &mut impl BufRead,
    output: &mut impl Write,
) -> Result<Settings, InitError> {
    let defaults = Settings::default();
    let question = format!(
        "Render mode ({}) [{}]: ",
        Mode::names(),
        defaults.mode.name()
    );
    let mode = parse_mode(&ask(input, output, &question)?, defaults.mode)?;
    let question = format!("Width in columns [{}]: ", defaults.width);
    let width = parse_width(&ask(input, output, &question)?, defaults.width)?;
    let mut shown = defaults.shown;
    for (show, name) in shown.iter_mut().zip(variable_names()) {
        let question = format!("Show {{{name}}}? [Y/n]: ");
        *show = parse_yes_no(&ask(input, output, &question)?, *show)?;
    }
    Ok(Settings { mode, width, shown })
}

/// Writes `question` to `output` with no newline, then reads one line of
/// `input`: the answer, trimmed, empty at the end of input.
fn ask(
    input: &mut impl BufRead,
    output: &mut impl Write,
    question: &str,
) -> Result<String, InitError> {
    output
        .write_all(question.as_bytes())
        .and_then(|()| output.flush())
        .map_err(InitError::Prompt)?;
    let mut answer = String::new();
    input.read_line(&mut answer).map_err(InitError::Prompt)?;
    Ok(answer.trim().to_owned())
}

/// The mode `answer` names, `default` when it is empty.
fn parse_mode(answer: &str, default: Mode) -> Result<Mode, InitError> {
    if answer.is_empty() {
        return Ok(default);
    }
    Mode::from_name(answer).ok_or_else(|| InitError::InvalidMode(answer.to_owned()))
}

/// The whole number of at least 1 in `answer`, `default` when it is empty.
fn parse_width(answer: &str, default: u32) -> Result<u32, InitError> {
    if answer.is_empty() {
        return Ok(default);
    }
    answer
        .parse()
        .ok()
        .filter(|&width| width >= 1)
        .ok_or_else(|| InitError::InvalidWidth(answer.to_owned()))
}

/// Whether `answer` is y or yes rather than n or no, in any case, `default`
/// when it is empty.
fn parse_yes_no(answer: &str, default: bool) -> Result<bool, InitError> {
    if answer.is_empty() {
        Ok(default)
    } else if is_yes(answer) {
        Ok(true)
    } else if answer.eq_ignore_ascii_case("n") || answer.eq_ignore_ascii_case("no") {
        Ok(false)
    } else {
        Err(InitError::InvalidAnswer(answer.to_owned()))
    }
}

/// Whether `answer` is y or yes, in any case.
fn is_yes(answer: &str) -> bool {
    answer.eq_ignore_ascii_case("y") || answer.eq_ignore_ascii_case("yes")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn image_under_home_starts_with_tilde() {
        let value = image_value(Path::new("/h/.config/jaci/art.ansi"), Path::new("/h"));

        assert_eq!(value.unwrap(), "~/.config/jaci/art.ansi");
    }

    #[test]
    fn image_elsewhere_stays_absolute() {
        let value = image_value(Path::new("/x/jaci/art.ansi"), Path::new("/h"));

        assert_eq!(value.unwrap(), "/x/jaci/art.ansi");
    }

    #[test]
    fn full_config_says_left_out_keys_take_their_default() {
        let text = full("~/art.ansi", &Settings::default());

        assert_eq!(
            text.lines().next(),
            Some("# jaci's config. Every key is optional; left out, it takes its default.")
        );
    }

    #[test]
    fn empty_answers_take_the_defaults() {
        assert_eq!(parse_mode("", Mode::Ascii).unwrap(), Mode::Ascii);
        assert_eq!(parse_width("", 7).unwrap(), 7);
        assert!(parse_yes_no("", true).unwrap());
    }

    #[test]
    fn answers_parse() {
        assert_eq!(
            parse_mode("braille", Mode::Halfblock).unwrap(),
            Mode::Braille
        );
        assert_eq!(parse_width("40", 80).unwrap(), 40);
        for (answer, shown) in [("y", true), ("YES", true), ("N", false), ("no", false)] {
            assert_eq!(parse_yes_no(answer, !shown).unwrap(), shown, "{answer}");
        }
    }

    #[test]
    fn invalid_answers_name_themselves() {
        let message = |result: Result<(), InitError>| result.unwrap_err().to_string();

        assert_eq!(
            message(parse_mode("Braille", Mode::Halfblock).map(drop)),
            "jaci: 'Braille' is not a valid render mode"
        );
        for width in ["0", "-3", "4.5", "wide"] {
            assert_eq!(
                message(parse_width(width, 80).map(drop)),
                format!("jaci: '{width}' is not a valid width")
            );
        }
        assert_eq!(
            message(parse_yes_no("maybe", true).map(drop)),
            "jaci: 'maybe' is not a valid answer"
        );
    }

    #[test]
    fn answers_are_trimmed_and_end_of_input_is_empty() {
        let mut input = "  braille \n".as_bytes();
        let mut output = Vec::new();

        assert_eq!(ask(&mut input, &mut output, "Q: ").unwrap(), "braille");
        assert_eq!(ask(&mut input, &mut output, "Q: ").unwrap(), "");
        assert_eq!(output, b"Q: Q: ");
    }

    #[test]
    fn only_yes_confirms_overwrite() {
        let path = Path::new("/c/config.toml");
        for (answer, confirmed) in [(" Yes\n", true), ("y", true), ("n\n", false), ("", false)] {
            let mut output = Vec::new();

            let result = confirm_overwrite(&mut answer.as_bytes(), &mut output, path);

            assert_eq!(result.is_ok(), confirmed, "{answer:?}");
        }
    }
}
