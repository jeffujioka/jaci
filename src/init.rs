//! `jaci init`'s config templates, the full one and the minimal one, and the
//! questions it asks on stdin and stdout.

use std::error::Error;
use std::fmt;
use std::io::{self, BufRead, Write};
use std::path::{Path, PathBuf};

use crate::config::{
    self, Align, Block, BlockText, DEFAULT_PADDING, Encode, HOME_PREFIX, Kind, Style, TextBlock,
    Valign,
};
use crate::named::Named;
use crate::render::{AsciiStyle, DEFAULT_CHARSET, Mode};
use crate::template;

/// The variables interactive init asks about, in order, each as
/// `Show <variable>? [Y/n]: `; a no drops the blocks that need its yes.
const QUESTIONS: [&str; 7] = [
    "{hostname}",
    "{os}",
    "{cpu}",
    "{gpu}",
    "{ram}",
    "{disk}",
    "{uptime}",
];
const HOSTNAME: usize = 0;
const OS: usize = 1;
const CPU: usize = 2;
const GPU: usize = 3;
const RAM: usize = 4;
const DISK: usize = 5;
const UPTIME: usize = 6;

/// The content of the minimal config's one block.
const MINIMAL_CONTENT: &str = "Host: {hostname}";

/// What a block of the full config shows.
#[derive(Clone, Copy)]
enum Look {
    /// A `title` block.
    Title,
    /// A text block: its label, its icon and what it shows after them.
    Row {
        label: &'static str,
        icon: char,
        content: &'static str,
    },
    /// A `dots` block.
    Dots,
}

/// The answers that keep a block of the full config, each question an index
/// into [`QUESTIONS`].
#[derive(Clone, Copy)]
enum Keep {
    /// None: the block is always written.
    Always,
    /// A yes to the question.
    Yes(usize),
    /// Yes to `{cpu}` and to `{gpu}`, which have the same value.
    SameChip,
    /// A yes to the question, `{cpu}` or `{gpu}`, unless [`Keep::SameChip`]
    /// keeps the chip block in its place.
    OwnChip(usize),
}

/// A block of the full config and the answers that keep it.
struct Entry {
    look: Look,
    keep: Keep,
}

/// The entry of a text block labelled `label` with `icon`, showing
/// `content`.
const fn row(label: &'static str, icon: char, content: &'static str, keep: Keep) -> Entry {
    Entry {
        look: Look::Row {
            label,
            icon,
            content,
        },
        keep,
    }
}

/// The blocks the full config can hold, in order (D13), with the Nerd Font
/// icons of D14.
const ENTRIES: [Entry; 11] = [
    Entry {
        look: Look::Title,
        keep: Keep::Yes(HOSTNAME),
    },
    row("os", '\u{F108}', "{os}", Keep::Yes(OS)),
    row("chip", '\u{F2DB}', "{cpu}", Keep::SameChip),
    row("cpu", '\u{F2DB}', "{cpu}", Keep::OwnChip(CPU)),
    row("gpu", '\u{F26C}', "{gpu}", Keep::OwnChip(GPU)),
    row("ram", '\u{F0E4}', "{ram_meter}", Keep::Yes(RAM)),
    row("disk", '\u{F0A0}', "{disk_meter}", Keep::Yes(DISK)),
    row("up", '\u{F254}', "{uptime}", Keep::Yes(UPTIME)),
    row("date", '\u{F073}', "{date}", Keep::Always),
    row("time", '\u{F017}', "{time} · {week}", Keep::Always),
    Entry {
        look: Look::Dots,
        keep: Keep::Always,
    },
];

/// The entries of [`ENTRIES`] that `settings` keep, in order. Only when
/// `settings` keep both `{cpu}` and `{gpu}` is `cpu_and_gpu` called, for
/// their values, which choose the chip block or the cpu and gpu blocks
/// (D12).
fn entries<'a>(
    settings: &Settings,
    cpu_and_gpu: impl FnOnce() -> (&'a str, &'a str),
) -> Vec<&'static Entry> {
    let shown = settings.shown;
    let same_chip = shown[CPU] && shown[GPU] && {
        let (cpu, gpu) = cpu_and_gpu();
        cpu == gpu
    };
    ENTRIES
        .iter()
        .filter(|entry| match entry.keep {
            Keep::Always => true,
            Keep::Yes(question) => shown[question],
            Keep::SameChip => same_chip,
            Keep::OwnChip(question) => shown[question] && !same_chip,
        })
        .collect()
}

/// The blocks of the full config [`full`] writes for `settings` and
/// `cpu_and_gpu`, in order.
pub fn blocks<'a>(
    settings: &Settings,
    cpu_and_gpu: impl FnOnce() -> (&'a str, &'a str),
) -> Vec<Block> {
    entries(settings, cpu_and_gpu)
        .into_iter()
        .map(|entry| match entry.look {
            Look::Title => Block::Title,
            Look::Row {
                label,
                icon,
                content,
            } => Block::Text(TextBlock {
                text: BlockText::Content(
                    template::parse(content).expect("init's contents are templates"),
                ),
                style: Style::default(),
                color: None,
                label: Some(label.to_owned()),
                icon: Some(icon.to_string()),
            }),
            Look::Dots => Block::Dots,
        })
        .collect()
}

/// What the full config sets that init can ask for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Settings {
    pub mode: Mode,
    pub ascii_style: AsciiStyle,
    pub width: u32,
    /// Whether each question of [`QUESTIONS`] is answered yes, in their
    /// order.
    pub shown: [bool; QUESTIONS.len()],
}

impl Default for Settings {
    /// The config defaults, every question answered yes.
    fn default() -> Self {
        let encode = Encode::default();
        Self {
            mode: encode.mode,
            ascii_style: encode.ascii_style,
            width: encode.width,
            shown: [true; QUESTIONS.len()],
        }
    }
}

impl Settings {
    /// The `[encode]` keys a config written with these settings holds: their
    /// mode, ascii style and width, every other key its default.
    pub fn encode(&self) -> Encode {
        Encode {
            mode: self.mode,
            ascii_style: self.ascii_style,
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
    /// The ascii style answer is not a style's name.
    InvalidStyle(String),
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
            Self::InvalidStyle(answer) => write!(f, "jaci: '{answer}' is not a valid ascii style"),
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
            | Self::InvalidStyle(_)
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
/// `[banner].image` set to `image`, the mode, ascii style and width of
/// `settings`, the blocks [`blocks`] makes of `settings` and `cpu_and_gpu`,
/// and commented-out examples of the other block keys.
pub fn full<'a>(
    image: &str,
    settings: &Settings,
    cpu_and_gpu: impl FnOnce() -> (&'a str, &'a str),
) -> String {
    let encode = settings.encode();
    let mut blocks = String::new();
    let (mut first_kind, mut first_row) = (true, true);
    for entry in entries(settings, cpu_and_gpu) {
        blocks.push_str("[[banner.text.blocks]]\n");
        let kind = match entry.look {
            Look::Title => Some(Kind::Title),
            Look::Dots => Some(Kind::Dots),
            Look::Row { .. } => None,
        };
        if let Some(kind) = kind {
            if first_kind {
                blocks.push_str(&format!(
                    "# The block's kind: {}. A title shows the user at the\n\
                     # host over a rule, dots one dot per art colour; neither holds other keys.\n",
                    Kind::one_of()
                ));
                first_kind = false;
            }
            blocks.push_str(&format!("kind = {}\n", toml_value(kind.name())));
        }
        if let Look::Row {
            label,
            icon,
            content,
        } = entry.look
        {
            if first_row {
                blocks.push_str("# A label and icon start the block, in the art's colours.\n");
            }
            blocks.push_str(&format!("label = {}\n", toml_value(label)));
            if first_row {
                blocks.push_str(
                    "# The icon: a Nerd Font code point, found on the Nerd Font cheat sheet\n\
                     # (nerdfonts.com/cheat-sheet).\n",
                );
            }
            // As a TOML escape, so the file reads well without a Nerd Font.
            blocks.push_str(&format!("icon = \"\\u{:04X}\"\n", u32::from(icon)));
            if first_row {
                blocks.push_str(
                    "# The text, with variables such as {hostname} and {cmd:...} for a\n\
                     # command's output.\n",
                );
            }
            blocks.push_str(&format!("content = {}\n", toml_value(content)));
            if first_row {
                blocks.push_str(&format!(
                    "# The style: {}.\nstyle = {}\n",
                    Style::one_of(),
                    toml_value(Style::default().name())
                ));
                first_row = false;
            }
        }
        blocks.push('\n');
    }
    format!(
        "\
# jaci's config. Every key is optional; left out, it takes its default.

[encode]
# The render mode: {modes}.
mode = {mode}
# How ascii mode picks and colours its characters: {styles}.
ascii_style = {style}
# The art's width in columns: a whole number of at least 1.
width = {width}
# The narrowest art the banner draws, in columns; a smaller pane shows the
# text panel alone.
min_width = {min_width}
# The widest art the banner draws, in columns.
max_width = {max_width}
# Character width ÷ height, which keeps the art's proportions: a number above 0.
ratio = {ratio}
# The characters ascii mode draws with, darkest first; left out, the ascii
# style's own.
# charset = {charset}

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
# Icons need a Nerd Font: false switches them off when they show as boxes.
icons = true

# The text panel: its blocks, in order. A text block, the default kind, holds
# exactly one of content and command.
{blocks}\
# A block can show a command's output inside its text:
# [[banner.text.blocks]]
# label = \"shell\"
# content = {cmd_example}
#
# or as the whole block, run through sh -c with a 3-second limit:
# [[banner.text.blocks]]
# label = \"day\"
# command = {command_example}
# The colour as #RRGGBB; left out, the text keeps the terminal's colour. On a
# block with a label or icon, it colours the label, not the text.
# color = {color_example}
",
        modes = Mode::one_of(),
        mode = toml_value(encode.mode.name()),
        styles = AsciiStyle::one_of(),
        style = toml_value(encode.ascii_style.name()),
        width = toml_value(i64::from(encode.width)),
        min_width = toml_value(i64::from(encode.min_width)),
        max_width = toml_value(i64::from(encode.max_width)),
        ratio = toml_value(encode.ratio),
        charset = toml_value(DEFAULT_CHARSET),
        image = toml_value(image),
        padding = toml_value(i64::from(DEFAULT_PADDING)),
        aligns = Align::one_of(),
        align = toml_value(Align::default().name()),
        valigns = Valign::one_of(),
        valign = toml_value(Valign::Center.name()),
        cmd_example = toml_value("{cmd:basename \"$SHELL\"}"),
        command_example = toml_value("date '+%A, %d %B'"),
        color_example = toml_value("#87AFFF"),
    )
}

/// The minimal config: `[banner].image` set to `image` and the one block of
/// [`MINIMAL_CONTENT`].
pub fn minimal(image: &str) -> String {
    format!(
        "[banner]\nimage = {}\n\n[[banner.text.blocks]]\ncontent = {}\n",
        toml_value(image),
        toml_value(MINIMAL_CONTENT)
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

/// Asks for the render mode, the ascii style when that mode is ascii, the
/// width, then each block's question, an empty answer taking the default.
///
/// # Errors
///
/// [`InitError::InvalidMode`], [`InitError::InvalidStyle`],
/// [`InitError::InvalidWidth`] or [`InitError::InvalidAnswer`] at the first
/// answer that is not valid, and [`InitError::Prompt`] when a question cannot
/// be asked.
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
    let ascii_style = if mode == Mode::Ascii {
        let question = format!(
            "Ascii style ({}) [{}]: ",
            AsciiStyle::names(),
            defaults.ascii_style.name()
        );
        parse_style(&ask(input, output, &question)?, defaults.ascii_style)?
    } else {
        defaults.ascii_style
    };
    let question = format!("Width in columns [{}]: ", defaults.width);
    let width = parse_width(&ask(input, output, &question)?, defaults.width)?;
    let mut shown = defaults.shown;
    for (show, variable) in shown.iter_mut().zip(QUESTIONS) {
        let question = format!("Show {variable}? [Y/n]: ");
        *show = parse_yes_no(&ask(input, output, &question)?, *show)?;
    }
    Ok(Settings {
        mode,
        ascii_style,
        width,
        shown,
    })
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

/// The ascii style `answer` names, `default` when it is empty.
fn parse_style(answer: &str, default: AsciiStyle) -> Result<AsciiStyle, InitError> {
    if answer.is_empty() {
        return Ok(default);
    }
    AsciiStyle::from_name(answer).ok_or_else(|| InitError::InvalidStyle(answer.to_owned()))
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
        let text = full("~/art.ansi", &Settings::default(), || ("cpu", "gpu"));

        assert_eq!(
            text.lines().next(),
            Some("# jaci's config. Every key is optional; left out, it takes its default.")
        );
    }

    #[test]
    fn empty_answers_take_the_defaults() {
        assert_eq!(parse_mode("", Mode::Ascii).unwrap(), Mode::Ascii);
        assert_eq!(
            parse_style("", AsciiStyle::Dense).unwrap(),
            AsciiStyle::Dense
        );
        assert_eq!(parse_width("", 7).unwrap(), 7);
        assert!(parse_yes_no("", true).unwrap());
    }

    #[test]
    fn answers_parse() {
        assert_eq!(
            parse_mode("braille", Mode::Halfblock).unwrap(),
            Mode::Braille
        );
        assert_eq!(
            parse_style("tinted", AsciiStyle::Bright).unwrap(),
            AsciiStyle::Tinted
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
        assert_eq!(
            message(parse_style("Tinted", AsciiStyle::Bright).map(drop)),
            "jaci: 'Tinted' is not a valid ascii style"
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
