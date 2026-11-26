//! Text blocks resolved into the lines of the text panel: variables replaced,
//! commands run under a time limit, and each line styled.
//!
//! Layout, the panel's width, wrapping and alignment, is the compositor's.

use std::io::{self, Read as _};
use std::mem::MaybeUninit;
use std::os::unix::process::CommandExt as _;
use std::process::{Child, ChildStdout, Command, Stdio};
use std::sync::mpsc::{self, RecvTimeoutError, Sender};
use std::thread;
use std::time::{Duration, Instant};

use crate::cache::Cache;
use crate::config::{Block, BlockText, Style};
use crate::sysinfo::{DynamicVariable, StaticVariable, UNKNOWN};
use crate::template::{self, Segment};

/// The time each command gets from its own start before it is killed.
pub const COMMAND_TIME_LIMIT: Duration = Duration::from_secs(3);

const BOLD: &str = "\x1b[1m";
const DIM: &str = "\x1b[2m";
const RESET: &str = "\x1b[0m";

/// The panel lines of `blocks`, in block order: one line per line of each
/// block's resolved text, carrying the block's style and colour.
///
/// Static variables take their values from `cache`, [`UNKNOWN`] when it has
/// none; each dynamic variable a block uses is read once; any other `{...}`
/// stays as typed. Every command, inline or a block's `command`, starts
/// before any is waited on and gets [`COMMAND_TIME_LIMIT`] from its own
/// start; its value is its stdout less trailing newlines, or [`UNKNOWN`]
/// when it fails to start, exits non-zero, dies by a signal or runs out of
/// time. Variables are not replaced inside commands or their output.
pub fn resolve(blocks: &[Block], cache: &Cache) -> Vec<String> {
    let mut commands = Vec::new();
    for block in blocks {
        match &block.text {
            BlockText::Content(segments) => {
                commands.extend(segments.iter().filter_map(|segment| match segment {
                    Segment::Command(command) => Some(command.as_str()),
                    Segment::Text(_) => None,
                }));
            }
            BlockText::Command(command) => commands.push(command.as_str()),
        }
    }
    let mut outputs = run_all(&commands).into_iter();
    let mut output = || outputs.next().expect("one output per command");
    let mut variables = Variables::new(cache);
    let mut lines = Vec::new();
    for block in blocks {
        let text = match &block.text {
            BlockText::Content(segments) => {
                let mut text = String::new();
                for segment in segments {
                    match segment {
                        Segment::Text(literal) => variables.replace(literal, &mut text),
                        Segment::Command(_) => text.push_str(&output()),
                    }
                }
                text
            }
            BlockText::Command(_) => output(),
        };
        let prefix = style_prefix(block);
        lines.extend(text.lines().map(|line| {
            if prefix.is_empty() {
                line.to_owned()
            } else {
                format!("{prefix}{line}{RESET}")
            }
        }));
    }
    lines
}

/// The escapes that start each line of `block`: its style, then its colour.
fn style_prefix(block: &Block) -> String {
    let style = match block.style {
        Style::Normal => "",
        Style::Bold => BOLD,
        Style::Dim => DIM,
    };
    match block.color {
        Some([red, green, blue]) => format!("{style}\x1b[38;2;{red};{green};{blue}m"),
        None => style.to_owned(),
    }
}

/// The values of the variables of one [`resolve`] call.
struct Variables<'c> {
    cache: &'c Cache,
    /// The values of [`DynamicVariable::ALL`] read so far, by position.
    dynamic: [Option<String>; DynamicVariable::ALL.len()],
}

impl<'c> Variables<'c> {
    fn new(cache: &'c Cache) -> Self {
        Self {
            cache,
            dynamic: Default::default(),
        }
    }

    /// Appends `literal` to `text` with each placeholder of a variable
    /// replaced by its value; any other placeholder stays as typed.
    fn replace(&mut self, literal: &str, text: &mut String) {
        let mut copied = 0;
        for placeholder in template::placeholders(literal) {
            if let Some(value) = self.value(placeholder.name) {
                text.push_str(&literal[copied..placeholder.range.start]);
                text.push_str(value);
                copied = placeholder.range.end;
            }
        }
        text.push_str(&literal[copied..]);
    }

    /// The value of the variable `name`, `None` when no variable has that
    /// name. A dynamic variable is read on its first use only.
    fn value(&mut self, name: &str) -> Option<&str> {
        if let Some(variable) = StaticVariable::from_name(name) {
            return Some(self.cache.get(variable).unwrap_or(UNKNOWN));
        }
        let index = DynamicVariable::ALL
            .iter()
            .position(|variable| variable.name() == name)?;
        Some(self.dynamic[index].get_or_insert_with(|| DynamicVariable::ALL[index].resolve()))
    }
}

/// A started command, its stdout read by a thread of its own.
struct Running {
    child: Child,
    deadline: Instant,
}

/// The value of each of `commands`, in order: all of them start before any
/// is waited on, and each one still running at its deadline has its process
/// group killed.
fn run_all(commands: &[&str]) -> Vec<String> {
    let (sender, receiver) = mpsc::channel();
    let mut running: Vec<Option<Running>> = commands
        .iter()
        .enumerate()
        .map(|(index, command)| start(command, index, &sender))
        .collect();
    drop(sender);
    let mut values = vec![UNKNOWN.to_owned(); commands.len()];
    while let Some(deadline) = running.iter().flatten().map(|run| run.deadline).min() {
        match receiver.recv_timeout(deadline.saturating_duration_since(Instant::now())) {
            Ok((index, stdout)) => {
                let Some(run) = running[index].as_mut() else {
                    continue;
                };
                match run.child.try_wait() {
                    Ok(Some(status)) => {
                        running[index] = None;
                        if let (true, Some(stdout)) = (status.success(), stdout) {
                            values[index] = value_of(&stdout);
                        }
                    }
                    // The reader could not wait for the exit, so the command
                    // may still be running: its deadline still holds.
                    Ok(None) => {}
                    // The child cannot be waited on, so its id may name
                    // another group by now: leave it, unknown.
                    Err(_) => running[index] = None,
                }
            }
            Err(RecvTimeoutError::Timeout) => {
                let now = Instant::now();
                for slot in &mut running {
                    if let Some(run) = slot.take_if(|run| run.deadline <= now) {
                        stop(run);
                    }
                }
            }
            // Every reader is gone, so no running command can report.
            Err(RecvTimeoutError::Disconnected) => {
                running.iter_mut().flat_map(Option::take).for_each(stop)
            }
        }
    }
    values
}

/// Starts `command` through `sh -c` in a process group of its own, with a
/// thread that sends `index` and its stdout, `None` when unreadable, once
/// stdout is closed and the command has exited, or waiting for the exit
/// failed. `None` when it cannot start.
fn start(
    command: &str,
    index: usize,
    sender: &Sender<(usize, Option<Vec<u8>>)>,
) -> Option<Running> {
    let deadline = Instant::now() + COMMAND_TIME_LIMIT;
    let mut child = Command::new("sh")
        .arg("-c")
        .arg(command)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .process_group(0)
        .spawn()
        .ok()?;
    let stdout = child.stdout.take().expect("stdout is piped");
    let pid = child.id();
    let sender = sender.clone();
    let reader = thread::Builder::new().spawn(move || {
        let stdout = read_until_exit(stdout, pid);
        // The receiver is gone once the command was stopped or resolve ended.
        let _ = sender.send((index, stdout));
    });
    let run = Running { child, deadline };
    match reader {
        Ok(_) => Some(run),
        Err(_) => {
            stop(run);
            None
        }
    }
}

/// Reads `stdout` to its end, then blocks until the process `pid` has
/// exited without reaping it, so the process group it leads stays valid for
/// [`stop`] until its [`Child`] is waited on. Returns early when `waitid`
/// fails, so the receiver must not assume the process has exited.
fn read_until_exit(mut stdout: ChildStdout, pid: u32) -> Option<Vec<u8>> {
    let mut output = Vec::new();
    let read = stdout.read_to_end(&mut output);
    drop(stdout);
    let mut info = MaybeUninit::<libc::siginfo_t>::zeroed();
    loop {
        // SAFETY: `info` is valid for writes of one siginfo_t.
        let status = unsafe {
            libc::waitid(
                libc::P_PID,
                pid,
                info.as_mut_ptr(),
                libc::WEXITED | libc::WNOWAIT,
            )
        };
        if status == 0 || io::Error::last_os_error().kind() != io::ErrorKind::Interrupted {
            break;
        }
    }
    read.ok().map(|_| output)
}

/// Kills the process group of `run`, so commands its shell started die
/// too, and reaps its shell.
fn stop(mut run: Running) {
    if let Ok(group) = libc::pid_t::try_from(run.child.id()) {
        // SAFETY: killpg has no memory preconditions; the group is that of
        // the unreaped child, so its id names no other group.
        unsafe { libc::killpg(group, libc::SIGKILL) };
    }
    // An error means the child is already reaped, which leaves nothing to do.
    let _ = run.child.wait();
}

/// The value of a command whose stdout was `stdout`: the text, decoded as
/// UTF-8 with invalid bytes replaced, less its trailing newlines.
fn value_of(stdout: &[u8]) -> String {
    let decoded = String::from_utf8_lossy(stdout);
    let mut text: &str = &decoded;
    while let Some(line) = text.strip_suffix('\n') {
        text = line.strip_suffix('\r').unwrap_or(line);
    }
    text.to_owned()
}
