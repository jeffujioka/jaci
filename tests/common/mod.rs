//! What the binary tests share: a run of the built `jaci` with its config
//! folder and home folder in temporary folders, with piped stdio or with
//! stdout on a pseudo-terminal of a fixed size.

// Each test binary compiles this module whole and uses only part of it.
#![allow(dead_code)]

use std::fs::{self, File};
use std::io::{self, Read as _};
use std::os::fd::{FromRawFd as _, OwnedFd};
use std::path::PathBuf;
use std::process::{Command, ExitStatus, Output, Stdio};
use std::ptr;
use std::sync::{Mutex, MutexGuard, PoisonError};
use std::thread;
use std::time::{Duration, Instant};

use tempfile::TempDir;

/// The width of the pseudo-terminal [`in_terminal`] runs jaci on.
pub const TERMINAL_COLUMNS: u16 = 100;
/// The height of the pseudo-terminal [`in_terminal`] runs jaci on.
pub const TERMINAL_ROWS: u16 = 40;

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
pub struct Env {
    xdg_config_home: TempDir,
    home: TempDir,
}

impl Env {
    pub fn new() -> Self {
        Self {
            xdg_config_home: TempDir::new().unwrap(),
            home: TempDir::new().unwrap(),
        }
    }

    /// An env whose `config.toml` holds `toml`.
    pub fn with_config(toml: &str) -> Self {
        let env = Self::new();
        fs::create_dir_all(env.config_folder()).unwrap();
        fs::write(env.config_path(), toml).unwrap();
        env
    }

    pub fn config_folder(&self) -> PathBuf {
        self.xdg_config_home.path().join("jaci")
    }

    pub fn config_path(&self) -> PathBuf {
        self.config_folder().join("config.toml")
    }

    /// The command `jaci <args>`, run from the crate folder so
    /// `test_fixtures/` resolves, with this env's config and home folders.
    pub fn command(&self, args: &[&str]) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_jaci"));
        command
            .args(args)
            .current_dir(env!("CARGO_MANIFEST_DIR"))
            .env("XDG_CONFIG_HOME", self.xdg_config_home.path())
            .env("HOME", self.home.path());
        command
    }

    /// Runs `jaci <args>` with piped stdio.
    pub fn jaci(&self, args: &[&str]) -> Output {
        let mut command = self.command(args);
        command
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let spawns = lock_spawns();
        let child = command.spawn().unwrap();
        drop(spawns);
        child.wait_with_output().unwrap()
    }
}

pub fn stderr(output: &Output) -> String {
    String::from_utf8(output.stderr.clone()).unwrap()
}

/// How a run on a pseudo-terminal ended.
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

/// Runs `command` with its stdout on a new pseudo-terminal
/// [`TERMINAL_COLUMNS`] wide and [`TERMINAL_ROWS`] high, its stdin null and
/// its stderr piped, and reads the terminal until the child has exited and
/// closed it.
pub fn in_terminal(mut command: Command) -> TerminalRun {
    let spawns = lock_spawns();
    let (master, slave) = open_terminal();
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

/// A new pseudo-terminal's master and slave, both closed on exec. Call it
/// holding [`SPAWN`], so no child of another thread inherits them before
/// they are.
fn open_terminal() -> (OwnedFd, OwnedFd) {
    let mut size = libc::winsize {
        ws_row: TERMINAL_ROWS,
        ws_col: TERMINAL_COLUMNS,
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
