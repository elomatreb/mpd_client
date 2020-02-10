//! This module contains utilities for constructing MPD commands.

use std::borrow::Cow;
use std::convert::TryFrom;
use std::error::Error;
use std::fmt::{self, Debug};

use crate::filter::Filter;

/// Start a command list, separated with list terminators. Our parser can't separate messages when
/// the form of command list without terminators is used.
static COMMAND_LIST_BEGIN: &str = "command_list_ok_begin\n";

/// End a command list.
static COMMAND_LIST_END: &str = "command_list_end\n";

/// A command or a command list consisting of multiple commands, which can be sent to MPD.
///
/// If the command contains more than a single command, it is automatically wrapped in a command
/// list.
///
/// The primary way to create `Commands` is to use a [`CommandBuilder`](struct.ComandBuilder.html)
/// starting with the [`build`](#method.build) method.
///
/// Alternatively, a `TryFrom` implementation for strings is provided, which validates the given
/// command but does no further processing.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Command(Vec<String>);

/// Builder for [`Command`](struct.Command.html)s.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct CommandBuilder(Vec<CommandPart>, bool);

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
enum CommandPart {
    Command(String),
    Argument(String),
}

/// Error returned when attempting to construct an invalid command.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CommandError {
    /// Reason the command was invalid.
    pub reason: InvalidCommandReason,
    /// If given more than one command, the index of the first invalid command.
    pub list_at: Option<usize>,
}

/// Ways in which a command may be invalid.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InvalidCommandReason {
    /// The command was empty (either an empty command or an empty list commands).
    Empty,
    /// The command string contained an invalid character at the contained position. This is
    /// context-dependent, as some characters are only invalid in certain sections of a command.
    InvalidCharacter(usize, char),
    /// The element contained trailing or leading whitespace (whitespace in the middle of commands
    /// is used to separate arguments).
    UnncessaryWhitespace,
    /// Attempted to start or close a command list without using the provided methods.
    CommandList,
}

impl Command {
    /// Start a new `CommandBuilder`, starting with the given command.
    pub fn build(command: impl Into<String>) -> CommandBuilder {
        CommandBuilder(vec![CommandPart::Command(command.into())], false)
    }

    /// Render the command to the wire representation. Commands are automatically wrapped in
    /// command lists if necessary.
    pub fn render(mut self) -> String {
        if self.0.len() == 1 {
            let mut c = self.0.pop().unwrap();
            c.push('\n');
            c
        } else {
            assert!(self.0.len() >= 2);

            // A command list consists of a beginning, the list of commands, and an ending, all
            // terminated by newlines
            let mut out = String::with_capacity(
                COMMAND_LIST_BEGIN.len()
                    + self.0.iter().fold(0, |acc, c| acc + c.len() + 1)
                    + COMMAND_LIST_END.len(),
            );

            out.push_str(COMMAND_LIST_BEGIN);

            for c in self.0 {
                out.push_str(&c);
                out.push('\n');
            }

            out.push_str(COMMAND_LIST_END);

            out
        }
    }
}

impl CommandBuilder {
    /// Add an argument to the last command.
    ///
    /// The argument is automatically escaped and quoted if necessary, but you if you want to
    /// include nested data containing special characters (e.g. filter expressions), you may need
    /// to pre-escape them using [`escape_argument`](fn.escape_argument.html).
    pub fn argument(mut self, argument: impl Into<String>) -> Self {
        self.add_argument(argument);
        self
    }

    /// Add another command, starting a command list.
    pub fn command(mut self, command: impl Into<String>) -> Self {
        self.add_command(command);
        self
    }

    /// Add a filter expression as an argument to the last command.
    pub fn filter(mut self, filter: Filter) -> Self {
        self.add_filter(filter);
        self
    }

    /// Add an argument to the last command.
    ///
    /// Like [`argument`](#method.argument), but doesn't take `self` so it can be called in a loop.
    pub fn add_argument(&mut self, argument: impl Into<String>) {
        self.0.push(CommandPart::Argument(argument.into()));
    }

    /// Add a filter expresion as an argument to the last command.
    ///
    /// Like [`filter`](#method.filter), but doesn't take `self` so it can be called in a a loop.
    pub fn add_filter(&mut self, filter: Filter) {
        self.0.push(CommandPart::Argument(filter.render()));
    }

    /// Add another command, starting a command list.
    ///
    /// Like [`command`](#method.command), but doesn't take `self` so it can be called in a loop.
    pub fn add_command(&mut self, command: impl Into<String>) {
        self.0.push(CommandPart::Command(command.into()));
        self.1 = true;
    }

    /// Complete the command, validating all entered components.
    ///
    /// ```
    /// use mpd_protocol::Command;
    ///
    /// let c = Command::build("status");
    ///
    /// assert_eq!(
    ///     c.finish().unwrap().render(),
    ///     "status\n"
    /// );
    /// ```
    pub fn finish(self) -> Result<Command, CommandError> {
        let mut commands = Vec::new();
        let mut current_command = None;
        let mut command_index = 0;

        let is_list = self.1;

        for part in self.0 {
            match part {
                CommandPart::Command(mut c) => {
                    command_index += 1;

                    validate_command_part(&c).map_err(|mut e| {
                        if is_list {
                            e.list_at = Some(command_index - 1);
                        }

                        e
                    })?;

                    if is_command_list_command(&c) {
                        return Err(CommandError {
                            reason: InvalidCommandReason::CommandList,
                            list_at: if is_list {
                                Some(command_index - 1)
                            } else {
                                None
                            },
                        });
                    }

                    if let Some(command) = current_command {
                        commands.push(command);
                    }

                    c.make_ascii_lowercase();
                    current_command = Some(c);
                }
                CommandPart::Argument(a) => {
                    let a = validate_argument(&a).map_err(|mut e| {
                        if is_list {
                            e.list_at = Some(command_index - 1);
                        }

                        e
                    })?;

                    let a = escape_argument(a);
                    let needs_quotes = needs_quotes(&a);

                    let current = current_command.as_mut().unwrap();

                    // A command requires 1 byte for the leading space separator, and two
                    // additional bytes for quotes if necessary
                    current.reserve(1 + a.len() + if needs_quotes { 2 } else { 0 });
                    current.push(' ');

                    if needs_quotes {
                        current.push('"');
                    }

                    current.push_str(&a);

                    if needs_quotes {
                        current.push('"');
                    }
                }
            }
        }

        if let Some(c) = current_command {
            commands.push(c);
        }

        Ok(Command(commands))
    }

    /// Finish the command, panicking when invalid.
    pub fn unwrap(self) -> Command {
        self.finish().expect("Invalid command")
    }
}

impl TryFrom<&str> for Command {
    type Error = CommandError;

    fn try_from(c: &str) -> Result<Self, Self::Error> {
        let end_of_command_part = c.find(' ');

        let command_part = &c[..end_of_command_part.unwrap_or_else(|| c.len())];
        validate_command_part(command_part)?;

        validate_no_extra_whitespace(c)?;

        if let Some(i) = end_of_command_part {
            if let Some(space) = c[i..].chars().position(|c| c == '\n') {
                return Err(CommandError {
                    reason: InvalidCommandReason::InvalidCharacter(space, ' '),
                    list_at: None,
                });
            }
        }

        let mut done = c.to_owned();
        done[..end_of_command_part.unwrap_or_else(|| c.len())].make_ascii_lowercase();

        Ok(Self(vec![done]))
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
        Err(CommandError {
            reason: InvalidCommandReason::UnncessaryWhitespace,
            list_at: None,
        })
    } else {
        Ok(())
    }
}

fn validate_command_part(command: &str) -> Result<(), CommandError> {
    if command.is_empty() {
        return Err(CommandError {
            reason: InvalidCommandReason::Empty,
            list_at: None,
        });
    }

    validate_no_extra_whitespace(command)?;

    if let Some((i, c)) = command
        .char_indices()
        .find(|(_, c)| !is_valid_command_char(*c))
    {
        Err(CommandError {
            reason: InvalidCommandReason::InvalidCharacter(i, c),
            list_at: None,
        })
    } else {
        Ok(())
    }
}

/// Validate a
fn validate_argument(argument: &str) -> Result<&str, CommandError> {
    validate_no_extra_whitespace(argument)?;

    match argument.char_indices().find(|(_, c)| *c == '\n') {
        None => Ok(argument),
        Some((i, c)) => Err(CommandError {
            reason: InvalidCommandReason::InvalidCharacter(i, c),
            list_at: None,
        }),
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
        match self.reason {
            InvalidCommandReason::Empty => write!(f, "Command or command list was empty"),
            InvalidCommandReason::InvalidCharacter(i, c) => write!(
                f,
                "Command contained an invalid character: {:?} at position {}",
                c, i
            ),
            InvalidCommandReason::UnncessaryWhitespace => {
                write!(f, "Command contained leading or trailing whitespace")
            }
            InvalidCommandReason::CommandList => {
                write!(f, "Command attempted to open or close a command list")
            }
        }?;

        if let Some(i) = self.list_at {
            write!(f, " (at command list index {})", i)?;
        }

        Ok(())
    }
}
