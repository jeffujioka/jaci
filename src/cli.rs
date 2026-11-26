//! The `jaci` command line: its options, its commands and how their errors
//! reach the user.

use std::env;
use std::error::Error;
use std::fmt;
use std::fs;
use std::io::{self, IsTerminal, Write as _};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::thread;
use std::time::Duration;

use clap::builder::PossibleValue;
use clap::{Args, Parser, Subcommand, ValueEnum};

use crate::art_file::{self, ArtFileError};
use crate::atomic_file;
use crate::cache::{self, CacheError};
use crate::compose;
use crate::config::{self, Config, ConfigError, Encode};
use crate::image_input::{self, ImageInputError};
use crate::init::{self, InitError, Settings};
use crate::named::Named;
use crate::render::{self, AsciiStyle, Charset, Mode};
use crate::size::{self, Fit, Sizing, TooLarge};
use crate::text;

/// Follows every error jaci reports.
const HELP_LINE: &str = "Run 'jaci --help' or 'jaci init --help' for more details.";
const SPINNER_FRAMES: [char; 4] = ['|', '/', '-', '\\'];
const SPINNER_INTERVAL: Duration = Duration::from_millis(100);

/// Encode images into 24-bit colour ANSI art and show them as a
/// shell-startup banner.
#[derive(Debug, Parser)]
#[command(name = "jaci", version)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Encode an image into an art file.
    Encode(EncodeArgs),
    /// Write the system info cache: the values of the static variables the
    /// config's text blocks use.
    Cache,
    /// Show the art file with the text panel beside it, when stdout is a
    /// terminal.
    Banner(BannerArgs),
    /// Set jaci up: write the config, encode the image into the art file and
    /// write the system info cache.
    Init(InitArgs),
}

/// `jaci init`'s arguments.
#[derive(Debug, Args)]
struct InitArgs {
    /// The image: PNG, JPEG, WebP, BMP, GIF (its first frame) or TIFF.
    image: Option<PathBuf>,
    /// Write only [banner].image and one {hostname} block instead of the
    /// full, commented config.
    #[arg(long, conflicts_with = "interactive")]
    minimal: bool,
    /// Ask for the image when not given, the render mode, the width and
    /// which variables to show.
    #[arg(short, long)]
    interactive: bool,
}

/// `jaci banner`'s arguments.
#[derive(Debug, Args)]
struct BannerArgs {
    /// The art file to show [default: [banner].image, else
    /// <config folder>/art.ansi].
    art: Option<PathBuf>,
    /// Write the system info cache first, as `jaci cache` does.
    #[arg(long)]
    refresh_cache: bool,
}

/// `jaci encode`'s arguments, each `None` or `false` when not given.
#[derive(Debug, Args)]
struct EncodeArgs {
    /// The image: PNG, JPEG, WebP, BMP, GIF (its first frame) or TIFF.
    image: PathBuf,
    /// The art file to write [default: [banner].image, else
    /// <config folder>/art.ansi].
    #[arg(short, long, value_name = "FILE")]
    output: Option<PathBuf>,
    /// The render mode [default: [encode].mode, else halfblock].
    #[arg(short, long)]
    mode: Option<Mode>,
    /// The art's width in columns [default: [encode].width, else 80].
    #[arg(
        short,
        long,
        value_name = "COLUMNS",
        value_parser = clap::value_parser!(u32).range(1..),
        conflicts_with = "scale"
    )]
    width: Option<u32>,
    /// The art's width as a percentage of the --full width.
    #[arg(
        short,
        long,
        value_name = "PERCENT",
        value_parser = clap::value_parser!(u32).range(1..)
    )]
    scale: Option<u32>,
    /// One column per cell of image pixels across.
    #[arg(long)]
    full: bool,
    /// As wide as the terminal jaci runs in.
    #[arg(long)]
    auto: bool,
    /// Character width ÷ height [default: [encode].ratio, else 0.5].
    #[arg(long, value_parser = parse_ratio)]
    ratio: Option<f64>,
    /// The ascii mode characters, darkest first [default: [encode].charset,
    /// else " .:-=+*#%@"].
    #[arg(long, value_parser = parse_charset)]
    charset: Option<Charset>,
}

impl ValueEnum for Mode {
    fn value_variants<'a>() -> &'a [Self] {
        Self::ALL
    }

    fn to_possible_value(&self) -> Option<PossibleValue> {
        Some(PossibleValue::new(self.name()))
    }
}

/// A finite number above 0.
fn parse_ratio(text: &str) -> Result<f64, String> {
    text.parse::<f64>()
        .ok()
        .filter(|&ratio| size::valid_ratio(ratio))
        .ok_or_else(|| "must be a number above 0".to_owned())
}

/// Text of at least one character.
fn parse_charset(text: &str) -> Result<Charset, String> {
    Charset::new(text).ok_or_else(|| "must be text of at least one character".to_owned())
}

/// An error jaci reports: its message, then the help line, on stderr and
/// exit 1.
#[derive(Debug)]
enum CliError {
    NoHome,
    Config(ConfigError),
    NoTerminal,
    Image(ImageInputError),
    Size(TooLarge),
    Write {
        path: PathBuf,
        source: io::Error,
    },
    Cache(CacheError),
    Art(ArtFileError),
    Print(io::Error),
    Init(InitError),
    /// Neither the art file nor the cache exists.
    FirstTime,
    /// The art file does not exist.
    NoArt,
    /// The cache does not exist or lacks a static variable the config uses.
    NoCache,
}

impl fmt::Display for CliError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoHome => f.write_str("jaci: cannot find the home folder; set HOME"),
            Self::Config(error) => error.fmt(f),
            Self::NoTerminal => f.write_str("jaci: --auto needs a terminal to measure"),
            Self::Image(error) => error.fmt(f),
            Self::Size(error) => error.fmt(f),
            Self::Write { path, source } => {
                write!(
                    f,
                    "jaci: cannot write art file {}: {source}",
                    path.display()
                )
            }
            Self::Cache(error) => error.fmt(f),
            Self::Art(error) => error.fmt(f),
            Self::Print(source) => write!(f, "jaci: cannot print the banner: {source}"),
            Self::Init(error) => error.fmt(f),
            Self::FirstTime => f.write_str(
                "First time? Welcome! jaci needs a quick setup — run 'jaci init' and you're good to go.",
            ),
            Self::NoArt => f.write_str(
                "Your banner needs an image! Run 'jaci encode <image>' to turn one into terminal art.",
            ),
            Self::NoCache => f.write_str("Almost there! Run 'jaci cache' to finish setting up."),
        }
    }
}

impl Error for CliError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::NoHome | Self::NoTerminal | Self::FirstTime | Self::NoArt | Self::NoCache => None,
            Self::Size(error) => error.source(),
            Self::Config(error) => error.source(),
            Self::Image(error) => error.source(),
            Self::Write { source, .. } | Self::Print(source) => Some(source),
            Self::Cache(error) => error.source(),
            Self::Art(error) => error.source(),
            Self::Init(error) => error.source(),
        }
    }
}

impl From<ConfigError> for CliError {
    fn from(error: ConfigError) -> Self {
        Self::Config(error)
    }
}

impl From<InitError> for CliError {
    fn from(error: InitError) -> Self {
        Self::Init(error)
    }
}

impl From<ImageInputError> for CliError {
    fn from(error: ImageInputError) -> Self {
        Self::Image(error)
    }
}

impl From<TooLarge> for CliError {
    fn from(error: TooLarge) -> Self {
        Self::Size(error)
    }
}

impl From<CacheError> for CliError {
    fn from(error: CacheError) -> Self {
        Self::Cache(error)
    }
}

impl From<ArtFileError> for CliError {
    fn from(error: ArtFileError) -> Self {
        Self::Art(error)
    }
}

/// Runs jaci with the process's arguments and environment. A usage error
/// exits 2 through clap; an error jaci reports prints its message and the
/// help line on stderr and exits 1.
pub fn main() -> ExitCode {
    let cli = Cli::parse();
    match run(cli) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error}\n{HELP_LINE}");
            ExitCode::FAILURE
        }
    }
}

fn run(cli: Cli) -> Result<(), CliError> {
    // A banner outside a terminal does nothing at all (D36).
    if matches!(cli.command, Command::Banner(_)) && !io::stdout().is_terminal() {
        return Ok(());
    }
    let home = env::home_dir().ok_or(CliError::NoHome)?;
    let folder = config::folder(env::var_os("XDG_CONFIG_HOME").as_deref(), &home);
    match cli.command {
        Command::Encode(args) => with_spinner(|| encode(&args, &config::load(&folder, &home)?)),
        Command::Cache => Ok(cache::refresh(
            &config::load(&folder, &home)?,
            &folder.join(cache::FILE_NAME),
        )?),
        Command::Banner(args) => banner(&args, &config::load(&folder, &home)?, &folder),
        Command::Init(args) => init(args, &folder, &home),
    }
}

/// Sets jaci up: asks what it has to on stdout and stdin, reading the image
/// as soon as its path is known, fits it with the settings the config will
/// hold, then writes the config, the art file encoded as `jaci encode`
/// encodes with the written config, and the cache.
fn init(args: InitArgs, folder: &Path, home: &Path) -> Result<(), CliError> {
    if args.image.is_none() && !args.interactive {
        return Err(InitError::NoImage.into());
    }
    let config_path = folder.join(config::FILE_NAME);
    // Every check runs before any file is written, so an unreadable image, or
    // one too large for the settings, writes none (D38). The image is read
    // before the settings are asked, so a wrong path stops init at once.
    let (image, settings) = {
        let mut input = io::stdin().lock();
        let mut output = io::stdout().lock();
        // Any entry counts, a dangling symlink too: the write would replace
        // the file it names.
        if fs::symlink_metadata(&config_path).is_ok() {
            init::confirm_overwrite(&mut input, &mut output, &config_path)?;
        }
        // Without an image, the check above leaves only interactive init.
        let image_path = match args.image {
            Some(image) => image,
            None => init::ask_image(&mut input, &mut output, home)?,
        };
        let image = image_input::read(&image_path)?;
        let settings = if args.interactive {
            init::ask_settings(&mut input, &mut output)?
        } else {
            Settings::default()
        };
        (image, settings)
    };
    // `--minimal` leaves the `[encode]` keys out, and as it rules out `-i`
    // its settings are their defaults.
    let encode = settings.encode();
    let fit = size::fit(&image, encode.mode, &config_sizing(&encode))?;
    let art = init::image_value(&folder.join(config::DEFAULT_IMAGE), home)?;
    let text = if args.minimal {
        init::minimal(&art)
    } else {
        init::full(&art, &settings)
    };
    atomic_file::write(&config_path, text.as_bytes()).map_err(|source| InitError::Write {
        path: config_path,
        source,
    })?;
    let config = config::load(folder, home)?;
    debug_assert_eq!(config.encode, encode, "the config holds the settings");
    write_art(&fit, encode.mode, &encode.charset, &config.banner.image)?;
    Ok(cache::refresh(&config, &folder.join(cache::FILE_NAME))?)
}

/// Prints the art beside the text panel, sized for the terminal, after
/// refreshing the cache when asked to and checking that the art file and
/// every static variable the config uses are there.
fn banner(args: &BannerArgs, config: &Config, folder: &Path) -> Result<(), CliError> {
    let cache_path = folder.join(cache::FILE_NAME);
    if args.refresh_cache {
        cache::refresh(config, &cache_path)?;
    }
    let art = match art_file::read(args.art.as_deref().unwrap_or(&config.banner.image)) {
        Ok(art) => Some(art),
        Err(ArtFileError::Io { source, .. }) if source.kind() == io::ErrorKind::NotFound => None,
        Err(error) => return Err(error.into()),
    };
    let (art, cache) = match (art, cache::read(&cache_path)?) {
        (None, None) => return Err(CliError::FirstTime),
        (None, Some(_)) => return Err(CliError::NoArt),
        (Some(_), None) => return Err(CliError::NoCache),
        (Some(art), Some(cache)) => (art, cache),
    };
    if cache::used_variables(config)
        .into_iter()
        .any(|variable| cache.get(variable).is_none())
    {
        return Err(CliError::NoCache);
    }
    let text = &config.banner.text;
    // The terminal jaci runs in: stdout, which the check above found to be
    // one. Without a size to read the art shows alone.
    let width = terminal_size::terminal_size().map_or(0, |(columns, _rows)| columns.0.into());
    let panel = compose::panel_width(art.columns(), config.banner.padding, width);
    // With no panel to show the text in, no block resolves and so no
    // command runs.
    let lines = match panel {
        Some(_) => text::resolve(&text.blocks, &cache),
        None => Vec::new(),
    };
    let rows = compose::banner(
        art.rows(),
        art.columns(),
        &lines,
        panel,
        text.align,
        text.valign,
    );
    let mut output = String::new();
    for row in rows {
        output.push_str(&row);
        output.push('\n');
    }
    io::stdout()
        .lock()
        .write_all(output.as_bytes())
        .map_err(CliError::Print)
}

/// Encodes `args.image` into an art file, the config filling in what the
/// options leave out.
fn encode(args: &EncodeArgs, config: &Config) -> Result<(), CliError> {
    // The terminal jaci runs in: stdout, else stderr, else stdin (D32).
    let auto = if args.auto {
        let (columns, _rows) = terminal_size::terminal_size().ok_or(CliError::NoTerminal)?;
        Some(columns.0)
    } else {
        None
    };
    let image = image_input::read(&args.image)?;
    let sizing = Sizing {
        full: args.full,
        auto,
        width: args.width,
        scale: args.scale,
        ratio: args.ratio,
        ..config_sizing(&config.encode)
    };
    let mode = args.mode.unwrap_or(config.encode.mode);
    let fit = size::fit(&image, mode, &sizing)?;
    write_art(
        &fit,
        mode,
        args.charset.as_ref().unwrap_or(&config.encode.charset),
        args.output.as_deref().unwrap_or(&config.banner.image),
    )
}

/// The sizing the `[encode]` keys give, with no options.
fn config_sizing(encode: &Encode) -> Sizing {
    Sizing {
        config_width: Some(encode.width),
        config_ratio: Some(encode.ratio),
        ..Sizing::default()
    }
}

/// Renders `fit`, fitted in `mode`, ascii with `charset`, and writes the art
/// file at `path`.
fn write_art(fit: &Fit<'_>, mode: Mode, charset: &Charset, path: &Path) -> Result<(), CliError> {
    let rows = match mode {
        Mode::Halfblock => render::halfblock(&fit.image),
        Mode::Ascii => render::ascii(&fit.image, AsciiStyle::Plain, charset),
        Mode::Braille => render::braille(&fit.image),
    };
    art_file::write(path, &rows).map_err(|source| CliError::Write {
        path: path.to_path_buf(),
        source,
    })
}

/// Runs `work` with a progress spinner on stdout while stdout is a terminal,
/// erasing it before returning.
fn with_spinner<T>(work: impl FnOnce() -> T) -> T {
    if !io::stdout().is_terminal() {
        return work();
    }
    let (stop, stopped) = mpsc::channel::<()>();
    thread::scope(|scope| {
        scope.spawn(move || spin(&stopped));
        let result = work();
        drop(stop);
        result
    })
}

/// Draws spinner frames on stdout until `stopped`'s sender is dropped, then
/// erases the spinner. The spinner is decoration only, so a failed write to
/// the terminal is ignored rather than turned into an error.
fn spin(stopped: &Receiver<()>) {
    let mut stdout = io::stdout();
    for frame in SPINNER_FRAMES.iter().cycle() {
        let _ = write!(stdout, "\r{frame}");
        let _ = stdout.flush();
        if stopped.recv_timeout(SPINNER_INTERVAL) != Err(RecvTimeoutError::Timeout) {
            break;
        }
    }
    let _ = write!(stdout, "\r \r");
    let _ = stdout.flush();
}
