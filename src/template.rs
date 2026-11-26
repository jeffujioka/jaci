//! Text block templates: a block's `content` split into literal text and the
//! inline commands written as `{cmd:...}`, and the `{name}` placeholders of
//! the literal text.

use std::error::Error;
use std::fmt;
use std::ops::Range;

const COMMAND_OPEN: &str = "{cmd:";

/// One piece of a template, in the order it appears.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Segment {
    /// Literal text, which may hold variables such as `{hostname}`.
    Text(String),
    /// The shell command of a `{cmd:...}`, without the braces and the `cmd:`.
    Command(String),
}

/// A `{cmd:` whose opening brace no later `}` balances.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UnclosedCommand;

impl fmt::Display for UnclosedCommand {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("unclosed {cmd:")
    }
}

impl Error for UnclosedCommand {}

/// Splits `content` into literal text and inline commands. An inline command
/// ends at the `}` that balances its opening brace, counting every brace, even
/// one inside quotes, so `{cmd:awk '{print $1}'}` runs `awk '{print $1}'`.
/// No segment is empty text.
///
/// # Errors
///
/// [`UnclosedCommand`] when a `{cmd:` is never balanced.
pub fn parse(content: &str) -> Result<Vec<Segment>, UnclosedCommand> {
    let mut segments = Vec::new();
    let mut rest = content;
    while let Some(start) = rest.find(COMMAND_OPEN) {
        if start > 0 {
            segments.push(Segment::Text(rest[..start].to_owned()));
        }
        let command = &rest[start + COMMAND_OPEN.len()..];
        let end = balancing_brace(command).ok_or(UnclosedCommand)?;
        segments.push(Segment::Command(command[..end].to_owned()));
        rest = &command[end + 1..];
    }
    if !rest.is_empty() {
        segments.push(Segment::Text(rest.to_owned()));
    }
    Ok(segments)
}

/// The byte index in `text` of the `}` that closes a brace opened just before
/// `text`.
fn balancing_brace(text: &str) -> Option<usize> {
    let mut depth = 1_usize;
    text.bytes().position(|byte| {
        match byte {
            b'{' => depth += 1,
            b'}' => depth -= 1,
            _ => {}
        }
        depth == 0
    })
}

/// A `{name}` in literal text, which a variable of that name replaces.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Placeholder<'a> {
    /// The byte range of the placeholder, braces included.
    pub range: Range<usize>,
    /// The text between the braces.
    pub name: &'a str,
}

/// The placeholders of the literal text `text`, in order: each `{` with a
/// `}` after it and no brace between them, so `{{cpu}}` holds `{cpu}`.
pub fn placeholders(text: &str) -> impl Iterator<Item = Placeholder<'_>> {
    text.match_indices('{').filter_map(move |(start, _)| {
        let inside = &text[start + 1..];
        let end = inside.find(['{', '}'])?;
        inside[end..].starts_with('}').then(|| Placeholder {
            range: start..start + end + 2,
            name: &inside[..end],
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nested_braces_stay_in_command() {
        let segments = parse("IP: {cmd:awk '{print $1}'} up").unwrap();

        assert_eq!(
            segments,
            [
                Segment::Text("IP: ".to_owned()),
                Segment::Command("awk '{print $1}'".to_owned()),
                Segment::Text(" up".to_owned()),
            ]
        );
    }

    #[test]
    fn placeholders_are_innermost_brace_pairs() {
        let text = "{os} {{cpu}} {gpu {} }";

        let found: Vec<_> = placeholders(text)
            .map(|placeholder| (placeholder.range, placeholder.name))
            .collect();

        assert_eq!(found, [(0..4, "os"), (6..11, "cpu"), (18..20, "")]);
    }
}
