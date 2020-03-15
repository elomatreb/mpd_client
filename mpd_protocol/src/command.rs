//! Tools for constructing MPD commands.
//!
//! For an overview of available commands, see the [MPD documentation].
//!
//! This does not perform any validations on commands beyond checking they appear well-formed, so
//! it should not be tied to any particular protocol version.
//!
//! [MPD documentation]: https://www.musicpd.org/doc/html/protocol.html#command-reference

use std::borrow::Cow;
use std::error::Error;
use std::fmt::{self, Debug};
use std::iter;

/// Start a command list, separated with list terminators. Our parser can't separate messages when
/// the form of command list without terminators is used.
static COMMAND_LIST_BEGIN: &str = "command_list_ok_begin\n";

/// End a command list.
static COMMAND_LIST_END: &str = "command_list_end\n";

/// A single command, possibly including arguments.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Command {
    base: Cow<'static, str>,
    args: Vec<Cow<'static, str>>,
}

/// A non-empty list of commands.
///
/// Commands will be automatically wrapped in a proper command list if necessary.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct CommandList {
    first: Command,
    tail: Vec<Command>,
}

/// Things which can be used as arguments for commands.
pub trait Argument {
    /// Return the string representation of the argument.
    ///
    /// This does not need to include escaping (except where it is necessary due to nesting of
    /// escaped values, such as in filter expressions) or quoting of values containing whitespace.
    fn render(self) -> Cow<'static, str>;
}

/// Error returned when attempting to construct an invalid command.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CommandError {
    /// The command was empty (either an empty command or an empty list commands).
    Empty,
    /// The command string contained an invalid character at the contained position. This is
    /// context-dependent, as some characters are only invalid in certain sections of a command.
    InvalidCharacter(usize, char),
    /// The element contained trailing or leading whitespace (whitespace in the middle of commands
    /// is used to separate arguments).
    UnncessaryWhitespace,
    /// Attempted to start or close a command list manually.
    CommandList,
}

impl Command {
    /// Start a new command.
    ///
    /// Same as [`build`], but panics on error instead of returning a result.
    ///
    /// [`build`]: #method.build
    pub fn new(command: impl Into<Cow<'static, str>>) -> Self {
        Self::build(command).expect("Invalid command")
    }

    /// Start a new command.
    ///
    /// # Errors
    ///
    /// Errors are returned when the command base is invalid (e.g. empty string or containing
    /// whitespace).
    pub fn build(command: impl Into<Cow<'static, str>>) -> Result<Self, CommandError> {
        let base = command.into();

        validate_command_part(&base)?;

        Ok(Self {
            base,
            args: Vec::new(),
        })
    }

    /// Add an argument to the command.
    ///
    /// Same as [`add_argument`], but returns `Self` and panics on error.
    ///
    /// [`add_argument`]: #method.add_argument
    pub fn argument(mut self, argument: impl Argument) -> Self {
        self.add_argument(argument).expect("Invalid argument");
        self
    }

    /// Add an argument to the command.
    ///
    /// # Errors
    ///
    /// Errors are returned when the argument is invalid (e.g. empty string or containing invalid
    /// characters such as newlines).
    pub fn add_argument(&mut self, argument: impl Argument) -> Result<(), CommandError> {
        let argument = argument.render();

        validate_argument(&argument)?;

        self.args.push(argument);
        Ok(())
    }

    /// Render this command to the wire representation.
    pub(crate) fn render(self) -> String {
        let mut out = self.base.into_owned();

        for arg in self.args {
            // Argumetns needs to be quoted if they contain whitespace
            let quote = needs_quotes(&arg);
            let arg = escape_argument(&arg);

            // Leading space + length of argument + two quotes if necessary + newline
            out.reserve(1 + arg.len() + if quote { 2 } else { 0 } + 1);
            out.push(' ');

            if quote {
                out.push('"');
            }

            out.push_str(&arg);

            if quote {
                out.push('"');
            }
        }

        out.push('\n');

        out
    }
}

#[allow(clippy::len_without_is_empty)]
impl CommandList {
    /// Create a command list from the given single command.
    ///
    /// Unless further commands are added, the command will not be wrapped into a list.
    pub fn new(first: Command) -> Self {
        Self {
            first,
            tail: Vec::new(),
        }
    }

    /// Add another command to the list.
    ///
    /// Same as [`add`], but takes and returns `self` for chaining.
    ///
    /// [`add`]: #method.add
    pub fn command(mut self, command: Command) -> Self {
        self.add(command);
        self
    }

    /// Add another command to the list.
    pub fn add(&mut self, command: Command) {
        self.tail.push(command);
    }

    /// Get the number of commands in this command list.
    ///
    /// This is never 0.
    pub fn len(&self) -> usize {
        1 + self.tail.len()
    }

    /// Render the command list to the wire representation.
    pub(crate) fn render(self) -> String {
        // If the list only contains a single command, don't wrap it into a command list
        if self.tail.is_empty() {
            return self.first.render();
        }

        let commands = iter::once(self.first)
            .chain(self.tail)
            .map(|c| c.render())
            .collect::<Vec<_>>();

        let mut out = String::with_capacity(
            COMMAND_LIST_BEGIN.len()
                + commands.iter().map(|c| c.len()).sum::<usize>()
                + COMMAND_LIST_END.len(),
        );

        out.push_str(COMMAND_LIST_BEGIN);
        out.extend(commands);
        out.push_str(COMMAND_LIST_END);

        out
    }
}

impl Extend<Command> for CommandList {
    fn extend<T: IntoIterator<Item = Command>>(&mut self, iter: T) {
        self.tail.extend(iter);
    }
}

impl Argument for String {
    fn render(self) -> Cow<'static, str> {
        Cow::Owned(self)
    }
}

impl Argument for &'static str {
    fn render(self) -> Cow<'static, str> {
        Cow::Borrowed(self)
    }
}

/// Escape a single argument, prefixing necessary characters (quotes and backslashes) with
/// backslashes.
///
/// Returns a borrowed `Cow` if the argument did not require escaping.
///
/// ```
/// use mpd_protocol::command::escape_argument;
///
/// assert_eq!(escape_argument("foo'bar\""), "foo\\'bar\\\"");
/// ```
pub fn escape_argument(argument: &str) -> Cow<'_, str> {
    let escape_count = argument.chars().filter(|c| should_escape(*c)).count();

    if escape_count == 0 {
        // The argument does not need to be quoted or escaped, return back an unmodified reference
        return Cow::Borrowed(argument);
    }

    let mut out = String::with_capacity(argument.len() + escape_count);

    for c in argument.chars() {
        if should_escape(c) {
            out.push('\\');
        }

        out.push(c);
    }

    Cow::Owned(out)
}

/// If the given argument needs to be surrounded with quotes (i.e. it contains spaces).
fn needs_quotes(arg: &str) -> bool {
    arg.chars().any(|c| c == ' ')
}

/// If the given character needs to be escaped
fn should_escape(c: char) -> bool {
    c == '\\' || c == '"' || c == '\''
}

fn validate_no_extra_whitespace(command: &str) -> Result<(), CommandError> {
    // If either the first or last character are whitespace we have leading or trailing whitespace
    if command.chars().nth(0).unwrap().is_ascii_whitespace()
        || command.chars().last().unwrap().is_ascii_whitespace()
    {
        Err(CommandError::UnncessaryWhitespace)
    } else {
        Ok(())
    }
}

fn validate_command_part(command: &str) -> Result<(), CommandError> {
    if command.is_empty() {
        return Err(CommandError::Empty);
    }

    validate_no_extra_whitespace(command)?;

    if let Some((i, c)) = command
        .char_indices()
        .find(|(_, c)| !is_valid_command_char(*c))
    {
        Err(CommandError::InvalidCharacter(i, c))
    } else if is_command_list_command(command) {
        Err(CommandError::CommandList)
    } else {
        Ok(())
    }
}

/// Validate a
fn validate_argument(argument: &str) -> Result<&str, CommandError> {
    validate_no_extra_whitespace(argument)?;

    match argument.char_indices().find(|(_, c)| *c == '\n') {
        None => Ok(argument),
        Some((i, c)) => Err(CommandError::InvalidCharacter(i, c)),
    }
}

/// Commands can consist of alphabetic chars and underscores
fn is_valid_command_char(c: char) -> bool {
    c.is_ascii_alphabetic() || c == '_'
}

/// Returns `true` if the given command would start or end a command list.
fn is_command_list_command(command: &str) -> bool {
    command.starts_with("command_list")
}

impl Error for CommandError {}

impl fmt::Display for CommandError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CommandError::Empty => write!(f, "Command or command list was empty"),
            CommandError::InvalidCharacter(i, c) => write!(
                f,
                "Command contained an invalid character: {:?} at position {}",
                c, i
            ),
            CommandError::UnncessaryWhitespace => {
                write!(f, "Command contained leading or trailing whitespace")
            }
            CommandError::CommandList => {
                write!(f, "Command attempted to open or close a command list")
            }
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::filter::Filter;

    #[test]
    fn single_render() {
        assert_eq!(Command::build("status").unwrap().render(), "status\n");

        assert_eq!(Command::new("pause").argument("1").render(), "pause 1\n");

        assert_eq!(
            Command::new("hello").argument("foo bar").render(),
            "hello \"foo bar\"\n"
        );

        assert_eq!(
            Command::new("hello").argument("foo's bar\"").render(),
            "hello \"foo\\'s bar\\\"\"\n"
        );

        assert_eq!(
            Command::new("find")
                .argument(Filter::equal("album", "hello world"))
                .render(),
            "find \"(album == \\\"hello world\\\")\"\n"
        );

        assert_eq!(
            Command::build(" hello").unwrap_err(),
            CommandError::UnncessaryWhitespace
        );

        assert_eq!(Command::build("").unwrap_err(), CommandError::Empty);

        assert_eq!(
            Command::build("hello world").unwrap_err(),
            CommandError::InvalidCharacter(5, ' ')
        );

        assert_eq!(
            Command::build("command_list_ok_begin").unwrap_err(),
            CommandError::CommandList
        );
    }

    #[test]
    fn command_list_render() {
        let starter = CommandList::new(Command::new("status"));

        assert_eq!(starter.clone().render(), "status\n");

        assert_eq!(
            starter
                .clone()
                .command(Command::new("hello").argument("world"))
                .render(),
            "command_list_ok_begin\nstatus\nhello world\ncommand_list_end\n"
        );
    }
}
