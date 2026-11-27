//! Text blocks resolved into the lines of the text panel: variables replaced,
//! commands run under a time limit, and each line styled.
//!
//! Layout, the panel's width, wrapping and alignment, is the compositor's.

use std::io::{self, Read as _};
use std::iter;
use std::mem::MaybeUninit;
use std::os::unix::process::CommandExt as _;
use std::process::{Child, ChildStdout, Command, Stdio};
use std::sync::mpsc::{self, RecvTimeoutError, Sender};
use std::thread;
use std::time::{Duration, Instant};

use unicode_width::UnicodeWidthChar;

use crate::ansi::{self, ActiveSgr, BOLD, DIM, RESET, Token};
use crate::cache::Cache;
use crate::config::{Block, BlockText, Style, Text};
use crate::named::Named;
use crate::sysinfo::{DynamicVariable, StaticVariable, UNKNOWN};
use crate::template::{self, Segment};

/// The time each command gets from its own start before it is killed.
pub const COMMAND_TIME_LIMIT: Duration = Duration::from_secs(3);

/// Panel text has a tab stop every this many columns.
const TAB_STOP: usize = 8;

/// The panel lines of the blocks of `text`, in block order.
///
/// A text block gives one line per line of its resolved text, carrying the
/// block's style and colour, then the SGR sequences of the block's earlier
/// lines since their last reset, so a colour set on one line of a command's
/// output stays on its later lines.
/// Trailing lines of command output that hold escapes and no characters,
/// such as the reset `tput sgr0` prints after a newline, are left out; a
/// line of literal text always stays.
///
/// A labelled block, one with a `label` or an `icon`, starts its first line
/// with its icon, padded to the widest icon of the panel, when `text.icons`
/// is on and a block of the panel has an icon, then a space, then its
/// label, all bold in its label colour, then spaces up to the value column:
/// two past the widest label. Its later lines start with spaces up to that
/// column. The k-th labelled block takes palette colour k, or its own
/// `color`, which then no longer colours its text.
///
/// A title block gives `{user}`, a dim `@` and `{hostname}`, bold in
/// palette colours 1 and 2, then a dim rule as wide as the widest line of
/// the panel and at most `panel_width`. A dots block gives an empty line,
/// then one `●` per palette colour, in palette order.
///
/// Palette colour i of a palette of n colours is colour ((i − 1) mod n) + 1;
/// `palette` is never empty, as [`palette::of`](crate::palette::of) gives.
///
/// Static variables take their values from `cache`, [`UNKNOWN`] when it has
/// none; each dynamic variable a block uses is read once; any other `{...}`
/// stays as typed. Every command, inline or a block's `command`, starts
/// before any is waited on and gets [`COMMAND_TIME_LIMIT`] from its own
/// start; its value is its stdout less trailing newlines, or [`UNKNOWN`]
/// when it fails to start, exits non-zero, dies by a signal or runs out of
/// time. Variables are not replaced inside commands or their output.
///
/// All of a block's text, literal text, variable values and output alike,
/// and a title's user and host, keep their SGR sequences and newlines, and
/// have every tab expanded to the next stop every 8 columns, counted from
/// where the text starts on its panel line; their other escape sequences
/// and control characters, C0, DEL and C1, are removed, so no text moves
/// the cursor over the art. A label and an icon are cleaned the same way,
/// and lose their newlines too, so each stays on its row.
pub fn resolve(text: &Text, cache: &Cache, palette: &[[u8; 3]], panel_width: usize) -> Vec<String> {
    let text_blocks = || {
        text.blocks.iter().filter_map(|block| match block {
            Block::Text(block) => Some(block),
            Block::Title | Block::Dots => None,
        })
    };
    let mut commands = Vec::new();
    for block in text_blocks() {
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
    // Each text block's icon and label, cleaned, in block order.
    let leads: Vec<_> = text_blocks()
        .map(|block| {
            (
                block.icon.as_deref().map(one_line),
                block.label.as_deref().map(one_line),
            )
        })
        .collect();
    let icon_width = leads
        .iter()
        .filter_map(|(icon, _)| icon.as_deref())
        .map(ansi::visible_width)
        .max()
        .filter(|_| text.icons);
    let label_width = leads
        .iter()
        .filter_map(|(_, label)| label.as_deref())
        .map(ansi::visible_width)
        .max()
        .unwrap_or(0);
    let mut leads = leads.into_iter();
    let value_column = icon_width.map_or(0, |width| width + 1) + label_width + 2;
    let colour = |index: usize| palette[index % palette.len()];
    let mut labels = 0;
    let mut variables = Variables::new(cache);
    let mut lines = Vec::new();
    // Where each title's rule goes, once the widest line is known.
    let mut rules = Vec::new();
    for block in &text.blocks {
        let block = match block {
            Block::Text(block) => block,
            Block::Title => {
                let mut title = String::new();
                push_name(&mut title, variables.of(StaticVariable::User), colour(0));
                title.push_str(DIM);
                title.push('@');
                title.push_str(RESET);
                push_name(
                    &mut title,
                    variables.of(StaticVariable::Hostname),
                    colour(1),
                );
                let mut resolved = PanelText::default();
                resolved.push(&title, Source::Literal);
                resolved.push_lines(&mut lines, "", "", "");
                rules.push(lines.len());
                lines.push(String::new());
                continue;
            }
            Block::Dots => {
                lines.push(String::new());
                let mut dots = String::new();
                for (index, &dot) in palette.iter().enumerate() {
                    if index > 0 {
                        dots.push(' ');
                    }
                    ansi::push_foreground(&mut dots, dot);
                    dots.push('●');
                    dots.push_str(RESET);
                }
                lines.push(dots);
                continue;
            }
        };
        let (icon, label) = leads.next().expect("one lead per text block");
        let mut resolved = PanelText::default();
        match &block.text {
            BlockText::Content(segments) => {
                for segment in segments {
                    match segment {
                        Segment::Text(literal) => variables.replace(literal, &mut resolved),
                        Segment::Command(_) => resolved.push(&output(), Source::Output),
                    }
                }
            }
            BlockText::Command(_) => resolved.push(&output(), Source::Output),
        }
        if label.is_none() && icon.is_none() {
            resolved.push_lines(&mut lines, "", "", &style_prefix(block.style, block.color));
            continue;
        }
        let label = label.as_deref().unwrap_or("");
        let mut lead = BOLD.to_owned();
        ansi::push_foreground(&mut lead, block.color.unwrap_or(colour(labels)));
        labels += 1;
        if let Some(width) = icon_width {
            let icon = icon.as_deref().unwrap_or("");
            lead.push_str(icon);
            lead.extend(iter::repeat_n(' ', width - ansi::visible_width(icon) + 1));
        }
        lead.push_str(label);
        lead.push_str(RESET);
        lead.extend(iter::repeat_n(
            ' ',
            label_width - ansi::visible_width(label) + 2,
        ));
        resolved.push_lines(
            &mut lines,
            &lead,
            &" ".repeat(value_column),
            &style_prefix(block.style, None),
        );
    }
    if !rules.is_empty() {
        let width = lines
            .iter()
            .map(|line| ansi::visible_width(line))
            .max()
            .unwrap_or(0)
            .min(panel_width);
        let rule = format!("{DIM}{}{RESET}", "─".repeat(width));
        for index in rules {
            lines[index].clone_from(&rule);
        }
    }
    lines
}

/// Appends `name` bold in `colour`, as a title shows the user and the host.
fn push_name(title: &mut String, name: &str, colour: [u8; 3]) {
    title.push_str(BOLD);
    ansi::push_foreground(title, colour);
    title.push_str(name);
    title.push_str(RESET);
}

/// `piece` cleaned as [`PanelText`] cleans literal text, less its newlines,
/// as a label or an icon shows on one row.
fn one_line(piece: &str) -> String {
    let mut cleaned = PanelText::default();
    cleaned.push(piece, Source::Literal);
    cleaned.text.retain(|c| c != '\n');
    cleaned.text
}

/// The escapes that start each line of a block's text: `style`, then
/// `colour`.
fn style_prefix(style: Style, colour: Option<[u8; 3]>) -> String {
    let mut prefix = match style {
        Style::Normal => String::new(),
        Style::Bold => BOLD.to_owned(),
        Style::Dim => DIM.to_owned(),
    };
    if let Some(colour) = colour {
        ansi::push_foreground(&mut prefix, colour);
    }
    prefix
}

/// Where a piece of panel text comes from, which decides whether its lines
/// always stay.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Source {
    /// A block's literal text or a variable's value.
    Literal,
    /// A command's output.
    Output,
}

/// A block's resolved text as it is built. SGR sequences and newlines stay,
/// each tab is expanded to spaces up to the next stop of every [`TAB_STOP`]
/// columns, counted from where the text of the line the tab lands in
/// starts, and every other escape sequence with its body, a lone ESC, and
/// every other control character, C0, DEL and C1, go, so the text moves no
/// cursor and leaves only characters and colours.
#[derive(Default)]
struct PanelText {
    text: String,
    /// The display width of the last line of `text`.
    column: usize,
    /// The length of `text` up to the end of its last literal piece, less
    /// the piece's trailing newlines, which end a line without making it
    /// literal; every byte after it is command output or a newline.
    literal_end: usize,
}

impl PanelText {
    /// Appends `piece`, which comes from `source`.
    fn push(&mut self, piece: &str, source: Source) {
        for token in ansi::tokens(piece) {
            let pushed = match token {
                Token::Escape(escape) if ansi::is_sgr(escape) => {
                    self.text.push_str(escape);
                    true
                }
                Token::Char('\n') => {
                    self.text.push('\n');
                    self.column = 0;
                    false
                }
                Token::Char('\t') => {
                    let spaces = TAB_STOP - self.column % TAB_STOP;
                    self.text.extend(iter::repeat_n(' ', spaces));
                    self.column += spaces;
                    true
                }
                Token::Char(c) if !c.is_control() => {
                    self.text.push(c);
                    self.column += c.width().unwrap_or(0);
                    true
                }
                Token::Escape(_) | Token::Char(_) => false,
            };
            if pushed && source == Source::Literal {
                self.literal_end = self.text.len();
            }
        }
    }

    /// Appends to `lines` one line per line of the text, as [`str::lines`]
    /// splits it (the text holds no carriage return), less its trailing
    /// lines of command output that hold escapes and no characters. The
    /// first line starts with `first`, each later one with `later`, then
    /// `prefix`, the block's style, then the SGR sequences still active
    /// from the lines before it, and ends with a reset when `prefix` is not
    /// empty.
    fn push_lines(&self, lines: &mut Vec<String>, first: &str, later: &str, prefix: &str) {
        let mut active = ActiveSgr::default();
        // How many lines there are up to the last one that stays.
        let mut shown = lines.len();
        let mut start = 0;
        for chunk in self.text.split_inclusive('\n') {
            let line = chunk.strip_suffix('\n').unwrap_or(chunk);
            let lead = if start == 0 { first } else { later };
            let mut resolved = String::with_capacity(
                lead.len() + prefix.len() + active.as_str().len() + line.len() + RESET.len(),
            );
            resolved.push_str(lead);
            resolved.push_str(prefix);
            resolved.push_str(active.as_str());
            resolved.push_str(line);
            if !prefix.is_empty() {
                resolved.push_str(RESET);
            }
            let (mut escapes, mut characters) = (false, false);
            for token in ansi::tokens(line) {
                match token {
                    Token::Escape(escape) => {
                        active.record(escape);
                        escapes = true;
                    }
                    Token::Char(_) => characters = true,
                }
            }
            lines.push(resolved);
            let from_output = start >= self.literal_end;
            if !(from_output && escapes && !characters) {
                shown = lines.len();
            }
            start += chunk.len();
        }
        lines.truncate(shown);
    }
}

/// The values of the variables of one [`resolve`] call.
struct Variables<'c> {
    cache: &'c Cache,
    /// The values of [`DynamicVariable::ALL`] read so far, each at its
    /// position there.
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
    fn replace(&mut self, literal: &str, text: &mut PanelText) {
        let mut copied = 0;
        for placeholder in template::placeholders(literal) {
            if let Some(value) = self.value(placeholder.name) {
                text.push(&literal[copied..placeholder.range.start], Source::Literal);
                text.push(value, Source::Literal);
                copied = placeholder.range.end;
            }
        }
        text.push(&literal[copied..], Source::Literal);
    }

    /// The value of the variable `name`, `None` when no variable has that
    /// name. A dynamic variable is read on its first use only.
    fn value(&mut self, name: &str) -> Option<&str> {
        if let Some(variable) = StaticVariable::from_name(name) {
            return Some(self.of(variable));
        }
        let variable = DynamicVariable::from_name(name)?;
        Some(self.dynamic[variable as usize].get_or_insert_with(|| variable.resolve()))
    }

    /// The value of `variable` in the cache, [`UNKNOWN`] when it has none.
    fn of(&self, variable: StaticVariable) -> &'c str {
        self.cache.get(variable).unwrap_or(UNKNOWN)
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
