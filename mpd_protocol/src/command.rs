//! Tools for constructing MPD commands.
//!
//! For an overview of available commands, see the [MPD documentation].
//!
//! This does not perform any validations on commands beyond checking they appear well-formed, so
//! it should not be tied to any particular protocol version.
//!
//! [MPD documentation]: https://www.musicpd.org/doc/html/protocol.html#command-reference

use bytes::{BufMut, BytesMut};

use std::borrow::Cow;
use std::error::Error;
use std::fmt::{self, Debug};
use std::iter;
use std::time::Duration;

/// Start a command list, separated with list terminators. Our parser can't separate messages when
/// the form of command list without terminators is used.
static COMMAND_LIST_BEGIN: &[u8] = b"command_list_ok_begin\n";

/// End a command list.
static COMMAND_LIST_END: &[u8] = b"command_list_end\n";

/// A single command, possibly including arguments.
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct Command {
    base: Cow<'static, str>,
    args: Vec<Cow<'static, str>>,
}

/// A non-empty list of commands.
///
/// Commands will be automatically wrapped in a proper command list if necessary.
#[derive(Clone, PartialEq, Eq, Hash)]
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
    /// Same as [`Command::build`], but panics on error instead of returning a result.
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
    /// Same as [`Command::add_argument`], but returns `Self` and panics on error.
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

        self.args.push(escape_argument_internal(argument, true));
        Ok(())
    }

    /// Get the expected length when this command is rendered to the wire representation
    fn rendered_length_hint(&self) -> usize {
        let mut len = self.base.len();

        len += self.args.len(); // One separating space for each argument
        len += self.args.iter().map(|a| a.len()).sum::<usize>(); // The actual arguments
        len += 1; // Terminating newline

        len
    }

    /// Render this command to the wire representation.
    fn render(self, dst: &mut BytesMut) {
        dst.extend_from_slice(self.base.as_bytes());

        for arg in self.args {
            dst.put_u8(b' ');
            dst.extend_from_slice(arg.as_bytes());
        }

        dst.put_u8(b'\n');
    }
}

impl Debug for Command {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.args.is_empty() {
            write!(f, "Command({:?})", self.base)
        } else {
            write!(f, "Command({:?}, ", self.base)?;
            f.debug_list().entries(&self.args).finish()?;
            write!(f, ")")
        }
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
    /// Same as [`CommandList::add`], but takes and returns `self` for chaining.
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
    pub(crate) fn render(self, dst: &mut BytesMut) {
        // If the list only contains a single command, don't wrap it into a command list
        if self.tail.is_empty() {
            dst.reserve(self.first.rendered_length_hint());
            self.first.render(dst);
        } else {
            let commands_len = iter::once(&self.first)
                .chain(self.tail.iter())
                .map(|c| c.rendered_length_hint())
                .sum::<usize>();

            dst.reserve(COMMAND_LIST_BEGIN.len() + commands_len + COMMAND_LIST_END.len());

            dst.extend_from_slice(COMMAND_LIST_BEGIN);
            for command in iter::once(self.first).chain(self.tail) {
                command.render(dst);
            }
            dst.extend_from_slice(COMMAND_LIST_END);
        }
    }
}

impl Debug for CommandList {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_list()
            .entries(iter::once(&self.first).chain(&self.tail))
            .finish()
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

impl Argument for Cow<'static, str> {
    fn render(self) -> Cow<'static, str> {
        self
    }
}

impl Argument for bool {
    fn render(self) -> Cow<'static, str> {
        Cow::Borrowed(match self {
            true => "1",
            false => "0",
        })
    }
}

impl Argument for Duration {
    /// Song durations in the format MPD expects. Will round to third decimal place.
    fn render(self) -> Cow<'static, str> {
        Cow::Owned(format!("{:.3}", self.as_secs_f64()))
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
    escape_argument_internal(Cow::Borrowed(argument), false)
}

/// Like escape_argument, but preserves the lifetime of a passed Cow and can quote if necessary
fn escape_argument_internal(argument: Cow<'_, str>, enable_quotes: bool) -> Cow<'_, str> {
    let needs_quotes = enable_quotes && argument.contains(&[' ', '\t'][..]);
    let escape_count = argument.chars().filter(|c| should_escape(*c)).count();

    if escape_count == 0 && !needs_quotes {
        // The argument does not need to be quoted or escaped, return back an unmodified reference
        argument
    } else {
        // The base length of the argument + a backslash for each escaped character + two quotes if
        // necessary
        let len = argument.len() + escape_count + if needs_quotes { 2 } else { 0 };
        let mut out = String::with_capacity(len);

        if needs_quotes {
            out.push('"');
        }

        for c in argument.chars() {
            if should_escape(c) {
                out.push('\\');
            }

            out.push(c);
        }

        if needs_quotes {
            out.push('"');
        }

        Cow::Owned(out)
    }
}

/// If the given character needs to be escaped
fn should_escape(c: char) -> bool {
    c == '\\' || c == '"' || c == '\''
}

fn validate_no_extra_whitespace(command: &str) -> Result<(), CommandError> {
    // If either the first or last character are whitespace we have leading or trailing whitespace
    if command.chars().next().unwrap().is_ascii_whitespace()
        || command.chars().next_back().unwrap().is_ascii_whitespace()
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
            CommandError::Empty => write!(f, "empty command"),
            CommandError::InvalidCharacter(i, c) => {
                write!(f, "invalid character {:?} at position {}", c, i)
            }
            CommandError::UnncessaryWhitespace => write!(f, "leading or trailing whitespace"),
            CommandError::CommandList => write!(f, "attempted to open or close a command list"),
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn single_render() {
        let buf = &mut BytesMut::with_capacity(100);

        Command::build("status").unwrap().render(buf);
        assert_eq!(buf, "status\n");
        buf.clear();

        Command::new("pause").argument("1").render(buf);
        assert_eq!(buf, "pause 1\n");
        buf.clear();

        Command::new("hello").argument("foo bar").render(buf);
        assert_eq!(buf, "hello \"foo bar\"\n");
        buf.clear();

        Command::new("hello").argument("foo's bar\"").render(buf);
        assert_eq!(buf, "hello \"foo\\'s bar\\\"\"\n");
        buf.clear();

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
        let buf = &mut BytesMut::with_capacity(100);
        let starter = CommandList::new(Command::new("status"));

        starter.clone().render(buf);
        assert_eq!(buf, "status\n");
        buf.clear();

        starter
            .command(Command::new("hello").argument("world"))
            .render(buf);
        assert_eq!(
            buf,
            "command_list_ok_begin\nstatus\nhello world\ncommand_list_end\n"
        );
        buf.clear();
    }

    #[test]
    fn argument_escaping() {
        assert_eq!(escape_argument("status"), "status");

        assert_eq!(escape_argument("Joe's"), "Joe\\'s");

        assert_eq!(escape_argument("hello\\world"), "hello\\\\world");
    }

    #[test]
    fn argument_rendering() {
        assert_eq!(true.render(), "1");
        assert_eq!(false.render(), "0");

        assert_eq!(Duration::from_secs(2).render(), "2.000");
        assert_eq!(Duration::from_secs_f64(2.34567).render(), "2.346");
    }
}
