//! Tools for constructing MPD commands.
//!
//! For an overview of available commands, see the [MPD documentation].
//!
//! This does not perform any validations on commands beyond checking they appear well-formed, so
//! it should not be tied to any particular protocol version.
//!
//! [MPD documentation]: https://www.musicpd.org/doc/html/protocol.html#command-reference

use std::{
    borrow::Cow,
    error::Error,
    fmt::{self, Debug},
    time::Duration,
};

use bytes::{BufMut, BytesMut};

/// Start a command list, separated with list terminators. Our parser can't separate messages when
/// the form of command list without terminators is used.
const COMMAND_LIST_BEGIN: &[u8] = b"command_list_ok_begin\n";

/// End a command list.
const COMMAND_LIST_END: &[u8] = b"command_list_end\n";

/// A single command, possibly including arguments.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Command(pub(crate) BytesMut);

impl Command {
    /// Start a new command.
    ///
    /// Same as [`Command::build`], but panics on error instead of returning a result.
    pub fn new(command: &str) -> Self {
        Self::build(command).expect("Invalid command")
    }

    /// Start a new command.
    ///
    /// # Errors
    ///
    /// Errors are returned when the command base is invalid (e.g. empty string or containing
    /// whitespace).
    pub fn build(command: &str) -> Result<Self, CommandError> {
        validate_command_part(command)?;
        Ok(Command(BytesMut::from(command)))
    }

    /// Add an argument to the command.
    ///
    /// Same as [`Command::add_argument`], but returns `Self` and panics on error.
    pub fn argument<A: Argument>(mut self, argument: A) -> Self {
        self.add_argument(argument).expect("Invalid argument");
        self
    }

    /// Add an argument to the command.
    ///
    /// # Errors
    ///
    /// Errors are returned when the argument is invalid (e.g. empty string or containing invalid
    /// characters such as newlines).
    pub fn add_argument<A: Argument>(&mut self, argument: A) -> Result<(), CommandError> {
        let len_without_arg = self.0.len();

        self.0.put_u8(b' ');
        argument.render(&mut self.0);

        if let Err(e) = validate_argument(&self.0[len_without_arg + 1..]) {
            // Remove added invalid part again
            self.0.truncate(len_without_arg);
            return Err(e);
        }

        Ok(())
    }
}

/// A non-empty list of commands.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct CommandList(pub(crate) Vec<Command>);

#[allow(clippy::len_without_is_empty)]
impl CommandList {
    /// Create a command list from the given single command.
    ///
    /// Unless further commands are added, the command will not be wrapped into a list.
    pub fn new(first: Command) -> Self {
        CommandList(vec![first])
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
        self.0.push(command);
    }

    /// Get the number of commands in this command list.
    ///
    /// This is never 0.
    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub(crate) fn render(mut self) -> BytesMut {
        if self.len() == 1 {
            let mut buf = self.0.pop().unwrap().0;
            buf.put_u8(b'\n');
            return buf;
        }

        // Calculate required length
        let required_length = COMMAND_LIST_BEGIN.len()
            + self.0.iter().map(|c| c.0.len() + 1).sum::<usize>()
            + COMMAND_LIST_END.len();

        let mut buf = BytesMut::with_capacity(required_length);

        buf.put_slice(COMMAND_LIST_BEGIN);
        for command in self.0 {
            buf.put_slice(&command.0);
            buf.put_u8(b'\n');
        }
        buf.put_slice(COMMAND_LIST_END);

        buf
    }
}

impl Extend<Command> for CommandList {
    fn extend<T: IntoIterator<Item = Command>>(&mut self, iter: T) {
        self.0.extend(iter);
    }
}

/// Escape a single argument, prefixing necessary characters (quotes and backslashes) with
/// backslashes.
///
/// Returns a borrowed [`Cow`] if the argument did not require escaping.
///
/// ```
/// # use mpd_protocol::command::escape_argument;
/// assert_eq!(escape_argument("foo'bar\""), "foo\\'bar\\\"");
/// ```
pub fn escape_argument(argument: &str) -> Cow<'_, str> {
    let needs_quotes = argument.contains(&[' ', '\t'][..]);
    let escape_count = argument.chars().filter(|c| should_escape(*c)).count();

    if escape_count == 0 && !needs_quotes {
        // The argument does not need to be quoted or escaped, return back an unmodified reference
        Cow::Borrowed(argument)
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

fn validate_command_part(command: &str) -> Result<(), CommandError> {
    if command.is_empty() {
        return Err(CommandError::Empty);
    }

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

/// Validate an argument.
fn validate_argument(argument: &[u8]) -> Result<(), CommandError> {
    match argument.iter().position(|&c| c == b'\n') {
        None => Ok(()),
        Some(i) => Err(CommandError::InvalidCharacter(i, '\n')),
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

/// Error returned when attempting to construct an invalid command.
#[derive(Debug)]
pub enum CommandError {
    /// The command was empty (either an empty command or an empty list commands).
    Empty,
    /// The command string contained an invalid character at the contained position. This is
    /// context-dependent, as some characters are only invalid in certain sections of a command.
    InvalidCharacter(usize, char),
    /// Attempted to start or close a command list manually.
    CommandList,
}

impl Error for CommandError {}

impl fmt::Display for CommandError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CommandError::Empty => write!(f, "empty command"),
            CommandError::InvalidCharacter(i, c) => {
                write!(f, "invalid character {:?} at position {}", c, i)
            }
            CommandError::CommandList => write!(f, "attempted to open or close a command list"),
        }
    }
}

/// Things which can be used as arguments for commands.
pub trait Argument {
    /// Render the argument into the command buffer.
    ///
    /// Spaces before/after arguments are inserted automatically, but values need to be escaped
    /// manually. See [`escape_argument`].
    fn render(&self, buf: &mut BytesMut);
}

impl<A> Argument for &A
where
    A: Argument + ?Sized,
{
    fn render(&self, buf: &mut BytesMut) {
        (*self).render(buf);
    }
}

impl Argument for String {
    fn render(&self, buf: &mut BytesMut) {
        let arg = escape_argument(self);
        buf.put_slice(arg.as_bytes());
    }
}

impl Argument for str {
    fn render(&self, buf: &mut BytesMut) {
        let arg = escape_argument(self);
        buf.put_slice(arg.as_bytes());
    }
}

impl Argument for Cow<'_, str> {
    fn render(&self, buf: &mut BytesMut) {
        let arg = escape_argument(self);
        buf.put_slice(arg.as_bytes());
    }
}

impl Argument for bool {
    fn render(&self, buf: &mut BytesMut) {
        buf.put_u8(if *self { b'1' } else { b'0' });
    }
}

impl Argument for Duration {
    /// Song durations in the format MPD expects. Will round to third decimal place.
    fn render(&self, buf: &mut BytesMut) {
        use std::fmt::Write;
        write!(buf, "{:.3}", self.as_secs_f64()).unwrap();
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn arguments() {
        let mut command = Command::new("foo");
        assert_eq!(command.0, "foo");

        command.add_argument("bar").unwrap();
        assert_eq!(command.0, "foo bar");
    }

    #[test]
    fn argument_escaping() {
        assert_eq!(escape_argument("status"), "status");
        assert_eq!(escape_argument("Joe's"), "Joe\\'s");
        assert_eq!(escape_argument("hello\\world"), "hello\\\\world");
        assert_eq!(escape_argument("foo bar"), r#""foo bar""#);
    }

    #[test]
    fn argument_rendering() {
        let mut buf = BytesMut::new();

        "foo\"bar".render(&mut buf);
        assert_eq!(buf, "foo\\\"bar");
        buf.clear();

        true.render(&mut buf);
        assert_eq!(buf, "1");
        buf.clear();

        false.render(&mut buf);
        assert_eq!(buf, "0");
        buf.clear();

        Duration::from_secs(2).render(&mut buf);
        assert_eq!(buf, "2.000");
        buf.clear();

        Duration::from_secs_f64(2.34567).render(&mut buf);
        assert_eq!(buf, "2.346");
        buf.clear();
    }
}
