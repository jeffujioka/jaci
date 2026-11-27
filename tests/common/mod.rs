//! What the tests share: a run of the built `jaci` with its config folder
//! and home folder in temporary folders, with piped stdio or with stdout on
//! a pseudo-terminal of a chosen size, the files such a run reads and
//! writes, and the banner's layout as `jaci banner` makes it.
//!
//! Each test binary compiles this module whole and uses only part of it, so
//! the items some binaries leave unused allow dead code.

use std::fs::{self, File};
use std::io::{self, Read as _, Write as _};
use std::os::fd::{FromRawFd as _, OwnedFd};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus, Output, Stdio};
use std::ptr;
use std::sync::{Mutex, MutexGuard, PoisonError};
use std::thread;
use std::time::{Duration, Instant};

use jaci::art_file::{self, Art};
use jaci::cache::{self, Cache};
use jaci::compose;
use jaci::config::{self, Align, Block, BlockText, Config, Style, TextBlock, Valign};
use jaci::template;
use tempfile::TempDir;

/// The width of the pseudo-terminal [`in_terminal`] runs jaci on.
const TERMINAL_COLUMNS: u16 = 100;
/// The height of the pseudo-terminal [`in_terminal`] runs jaci on.
const TERMINAL_ROWS: u16 = 40;
/// Follows every error jaci reports.
#[allow(dead_code)]
pub const HELP_LINE: &str = "Run 'jaci --help' or 'jaci init --help' for more details.";

/// Held from the creation of fds that are not yet closed on exec until the
/// child that should get them has been spawned, so no child of another test
/// thread inherits them: the pseudo-terminal between `openpty` and `fcntl`,
/// and std's stdio pipes, which on macOS are made close-on-exec only after
/// they are created.
static SPAWN: Mutex<()> = Mutex::new(());

/// [`SPAWN`], whose `()` no panic can leave inconsistent.
fn lock_spawns() -> MutexGuard<'static, ()> {
    SPAWN.lock().unwrap_or_else(PoisonError::into_inner)
}

/// A temporary `XDG_CONFIG_HOME` and `HOME` for one run of jaci.
#[allow(dead_code)]
pub struct Env {
    xdg_config_home: TempDir,
    home: TempDir,
}

impl Env {
    #[allow(dead_code)]
    pub fn new() -> Self {
        Self {
            xdg_config_home: TempDir::new().unwrap(),
            home: TempDir::new().unwrap(),
        }
    }

    /// An env whose `config.toml` holds `toml`.
    #[allow(dead_code)]
    pub fn with_config(toml: &str) -> Self {
        let env = Self::new();
        fs::create_dir_all(env.config_folder()).unwrap();
        fs::write(env.config_path(), toml).unwrap();
        env
    }

    #[allow(dead_code)]
    pub fn config_folder(&self) -> PathBuf {
        self.xdg_config_home.path().join("jaci")
    }

    #[allow(dead_code)]
    pub fn config_path(&self) -> PathBuf {
        self.config_folder().join("config.toml")
    }

    #[allow(dead_code)]
    pub fn home(&self) -> &Path {
        self.home.path()
    }

    /// The cache file in this env's config folder.
    #[allow(dead_code)]
    pub fn cache_path(&self) -> PathBuf {
        self.config_folder().join(cache::FILE_NAME)
    }

    /// The default art file in this env's config folder.
    #[allow(dead_code)]
    pub fn art_path(&self) -> PathBuf {
        self.config_folder().join(config::DEFAULT_IMAGE)
    }

    /// Loads this env's config, failing the test when it does not load.
    #[allow(dead_code)]
    pub fn config(&self) -> Config {
        config::load(&self.config_folder(), self.home()).unwrap_or_else(|error| panic!("{error}"))
    }

    /// The command `jaci <args>`, run from the crate folder so
    /// `test_fixtures/` resolves, with this env's config and home folders.
    #[allow(dead_code)]
    pub fn command(&self, args: &[&str]) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_jaci"));
        command
            .args(args)
            .current_dir(env!("CARGO_MANIFEST_DIR"))
            .env("XDG_CONFIG_HOME", self.xdg_config_home.path())
            .env("HOME", self.home.path());
        command
    }

    /// Runs `jaci <args>` with piped stdout and stderr and a null stdin.
    #[allow(dead_code)]
    pub fn jaci(&self, args: &[&str]) -> Output {
        self.run(args, None)
    }

    /// Runs `jaci <args>` with piped stdio, `input` on its stdin.
    #[allow(dead_code)]
    pub fn jaci_with_input(&self, args: &[&str], input: &str) -> Output {
        self.run(args, Some(input))
    }

    /// Runs `jaci <args>` with piped stdout and stderr, and stdin a pipe
    /// holding `input`, else null.
    fn run(&self, args: &[&str], input: Option<&str>) -> Output {
        let mut command = self.command(args);
        command
            .stdin(input.map_or_else(Stdio::null, |_| Stdio::piped()))
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let spawns = lock_spawns();
        let mut child = command.spawn().unwrap();
        drop(spawns);
        if let Some(input) = input {
            // Dropped at the end of the block, so jaci reads the end of input.
            let mut stdin = child.stdin.take().unwrap();
            match stdin.write_all(input.as_bytes()) {
                // jaci may stop before it reads all of its input.
                Err(error) if error.kind() != io::ErrorKind::BrokenPipe => {
                    panic!("writing jaci's stdin: {error}")
                }
                _ => {}
            }
        }
        child.wait_with_output().unwrap()
    }
}

#[allow(dead_code)]
pub fn stderr(output: &Output) -> String {
    String::from_utf8(output.stderr.clone()).unwrap()
}

#[allow(dead_code)]
pub fn stdout(output: &Output) -> String {
    String::from_utf8(output.stdout.clone()).unwrap()
}

/// Asserts that a run exited 0 with no output.
#[allow(dead_code)]
pub fn assert_quiet_success(output: &Output) {
    assert_eq!(output.status.code(), Some(0), "{}", stderr(output));
    assert!(output.stdout.is_empty(), "{:?}", output.stdout);
    assert!(output.stderr.is_empty(), "{}", stderr(output));
}

/// Reads the cache file at `path`, failing the test when it is missing or
/// damaged.
#[allow(dead_code)]
pub fn read_cache(path: &Path) -> Cache {
    cache::read(path)
        .unwrap_or_else(|error| panic!("{error}"))
        .expect("the cache file is missing")
}

/// Reads the art file at `path`, failing the test when it is not one.
#[allow(dead_code)]
pub fn read_art(path: &Path) -> Art {
    art_file::read(path).unwrap_or_else(|error| panic!("{error}"))
}

/// A config whose text blocks are the `content` of each of `contents`.
#[allow(dead_code)]
pub fn config_with(contents: &[&str]) -> String {
    contents
        .iter()
        .map(|content| format!("[[banner.text.blocks]]\ncontent = '{content}'\n"))
        .collect()
}

/// A normal block without colour whose `content` is `content`.
#[allow(dead_code)]
pub fn content_block(content: &str) -> Block {
    Block::Text(TextBlock {
        text: BlockText::Content(template::parse(content).unwrap()),
        style: Style::Normal,
        color: None,
        label: None,
        icon: None,
    })
}

/// The banner of `art`, `columns` wide, on a terminal `terminal_width`
/// columns wide, with the panel [`compose::panel_width`] gives, as
/// `jaci banner` lays it out.
#[allow(dead_code)]
pub fn banner(
    art: &[String],
    columns: usize,
    text: &[String],
    padding: u32,
    align: Align,
    valign: Valign,
    terminal_width: usize,
) -> Vec<String> {
    let panel = compose::panel_width(columns, padding, terminal_width);
    compose::banner(art, columns, text, panel, align, valign)
}

/// How a run on a pseudo-terminal ended.
#[allow(dead_code)]
#[derive(Debug)]
pub struct TerminalRun {
    pub status: ExitStatus,
    /// What the child wrote to the terminal, with the terminal's `\r\n`
    /// turned back into `\n`.
    pub stdout: String,
    pub stderr: String,
    /// From just before the spawn to the child's exit.
    pub elapsed: Duration,
}

/// Runs `command` as [`in_terminal_sized`] does, on a pseudo-terminal
/// [`TERMINAL_COLUMNS`] wide and [`TERMINAL_ROWS`] high.
#[allow(dead_code)]
pub fn in_terminal(command: Command) -> TerminalRun {
    in_terminal_sized(command, TERMINAL_COLUMNS, TERMINAL_ROWS)
}

/// Runs `command` with its stdout on a new pseudo-terminal `columns` wide
/// and `rows` high, its stdin null and its stderr piped, and reads the
/// terminal until the child has exited and closed it.
#[allow(dead_code)]
pub fn in_terminal_sized(mut command: Command, columns: u16, rows: u16) -> TerminalRun {
    let spawns = lock_spawns();
    let (master, slave) = open_terminal(columns, rows);
    command
        .stdin(Stdio::null())
        .stdout(Stdio::from(slave))
        .stderr(Stdio::piped());
    let started = Instant::now();
    let child = command.spawn().unwrap();
    drop(spawns);
    // The command holds the parent's copy of the slave; while it is open the
    // master never reports the end of the output.
    drop(command);
    let reader = thread::spawn(move || read_terminal(File::from(master)));
    let output = child.wait_with_output().unwrap();
    let elapsed = started.elapsed();
    let stdout = reader.join().unwrap();
    TerminalRun {
        status: output.status,
        stdout: String::from_utf8(stdout).unwrap().replace("\r\n", "\n"),
        stderr: String::from_utf8(output.stderr).unwrap(),
        elapsed,
    }
}

/// A new pseudo-terminal `columns` wide and `rows` high: its master and
/// slave, both closed on exec. Call it holding [`SPAWN`], so no child of
/// another thread inherits them before they are.
fn open_terminal(columns: u16, rows: u16) -> (OwnedFd, OwnedFd) {
    let mut size = libc::winsize {
        ws_row: rows,
        ws_col: columns,
        ws_xpixel: 0,
        ws_ypixel: 0,
    };
    let mut master = -1;
    let mut slave = -1;
    // SAFETY: the fd pointers are valid for writes, a null name and termios
    // are allowed, and `size` outlives the call. Linux takes the termios and
    // the size as const pointers and macOS as mut ones; mut ones suit both.
    let result = unsafe {
        libc::openpty(
            &raw mut master,
            &raw mut slave,
            ptr::null_mut(),
            ptr::null_mut(),
            &raw mut size,
        )
    };
    assert_eq!(result, 0, "openpty: {}", io::Error::last_os_error());
    // SAFETY: openpty succeeded, so both are open fds that nothing else owns.
    let fds = unsafe { (OwnedFd::from_raw_fd(master), OwnedFd::from_raw_fd(slave)) };
    for fd in [master, slave] {
        // SAFETY: `fd` is open, owned by `fds` above.
        let result = unsafe { libc::fcntl(fd, libc::F_SETFD, libc::FD_CLOEXEC) };
        assert_eq!(result, 0, "fcntl: {}", io::Error::last_os_error());
    }
    fds
}

/// Everything the terminal's slave side wrote, read from `master` until it
/// reports the end: end of file on macOS, `EIO` on Linux once every copy of
/// the slave is closed.
fn read_terminal(mut master: File) -> Vec<u8> {
    let mut output = Vec::new();
    let mut buffer = [0; 4096];
    loop {
        match master.read(&mut buffer) {
            Ok(0) => return output,
            Ok(read) => output.extend_from_slice(&buffer[..read]),
            Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
            Err(error) if error.raw_os_error() == Some(libc::EIO) => return output,
            Err(error) => panic!("reading the terminal: {error}"),
        }
    }
}
