//! This module contains utilities for constructing MPD commands.

use std::convert::{TryFrom, TryInto};
use std::error::Error;
use std::fmt::{self, Debug};

/// Start a command list, separated with list terminators. Our parser can't separate messages when
/// the form of command list without terminators is used.
static COMMAND_LIST_BEGIN: &str = "command_list_ok_begin\n";

/// End a command list.
static COMMAND_LIST_END: &str = "command_list_end\n";

/// A command or a command list consisting of multiple commands, which can be sent to MPD.
///
/// The primary way to create `Commands` is to use the various `TryFrom` implementations, or the
/// [`new`](#method.new) function (which panics instead of returning results).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Command {
    commands: Vec<String>,
}

/// The command was invalid.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CommandError {
    /// Reason the command was invalid.
    pub reason: InvalidCommandReason,
    /// If given a possible comand list, at which index in the list the error is.
    pub list_at: Option<usize>,
}

/// Ways in which a command may be invalid.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InvalidCommandReason {
    /// The command was empty (either an empty command or an empty list commands).
    Empty,
    /// The command string
    InvalidCharacter(usize, char),
    /// The contained trailing or leading whitespace (whitespace in the middle of commands is used to separate arguments).
    UnncessaryWhitespace,
    /// Attempted to start or close a command list while already in one.
    NestedCommandList,
}

impl Command {
    /// Create a new command, but panic instead of returning a `Result` when the conversion fails.
    ///
    /// This may be useful in cases where you supply known-good commands for simplicity.
    ///
    /// ```
    /// use mpd_protocol::Command;
    ///
    /// let command = Command::new("status");
    ///
    /// assert_eq!(command.render(), "status\n");
    /// ```
    ///
    /// Panics on invalid values:
    ///
    /// ```should_panic
    /// use mpd_protocol::Command;
    ///
    /// // This panics
    /// Command::new("invalid\ncommand");
    /// ```
    pub fn new<C>(c: C) -> Self
    where
        C: TryInto<Self>,
        <C as TryInto<Self>>::Error: Debug,
    {
        c.try_into().expect("invalid command")
    }

    /// Create a command from the given base command and a list of arguments.
    ///
    /// The arguments will be joined into a single command, and quoted and escaped if necessary.
    ///
    /// ```
    /// use mpd_protocol::Command;
    ///
    /// assert_eq!(
    ///     Command::from_parts("hello", &["foo", "bar baz", "Foo's Bar"][..]).unwrap().render(),
    ///     "hello foo \"bar baz\" \"Foo\\'s Bar\"\n"
    /// );
    /// ```
    pub fn from_parts(command: &str, arguments: &[&str]) -> Result<Self, CommandError> {
        let mut command = command.to_owned();

        for arg in arguments {
            if arg.is_empty() {
                continue;
            }

            let escape_chars = arg.chars().filter(|c| should_escape(*c)).count();
            let need_quotes = match arg.chars().find(|c| *c == ' ') {
                Some(_) => true,
                None => false,
            };

            let mut additional = 1 + arg.len() + escape_chars;
            if need_quotes {
                additional += 2;
            }

            command.reserve(additional);
            command.push(' ');

            if need_quotes {
                command.push('"');
            }

            if escape_chars == 0 {
                command.push_str(arg);
            } else {
                escape_single_argument(&mut command, arg);
            }

            if need_quotes {
                command.push('"');
            }
        }

        // Validate command with arguments assembled
        validate_single_command(&command)?;

        Ok(Self {
            commands: vec![command],
        })
    }

    /// Render the command to the wire representation. Commands are automatically wrapped in
    /// command lists if necessary.
    pub fn render(self) -> String {
        let mut out;

        if self.commands.len() == 1 {
            let c = self.commands.first().unwrap();

            out = String::with_capacity(c.len() + 1);

            out.push_str(c);
            out.push('\n');
        } else {
            assert!(self.commands.len() > 1);

            // A command list consists of a beginning, the list of commands, and an ending, all
            // terminated by newlines
            out = String::with_capacity(
                COMMAND_LIST_BEGIN.len()
                    + self.commands.iter().fold(0, |acc, c| acc + c.len() + 1)
                    + COMMAND_LIST_END.len(),
            );

            out.push_str(COMMAND_LIST_BEGIN);

            for c in self.commands {
                out.push_str(&c);
                out.push('\n');
            }

            out.push_str(COMMAND_LIST_END);
        }

        out
    }
}

impl TryFrom<&str> for Command {
    type Error = CommandError;

    fn try_from(c: &str) -> Result<Self, Self::Error> {
        let mut c = validate_single_command(c)?.to_owned();
        canonicalize_command(&mut c);

        Ok(Self { commands: vec![c] })
    }
}

impl TryFrom<&[&str]> for Command {
    type Error = CommandError;

    fn try_from(commands: &[&str]) -> Result<Self, Self::Error> {
        if commands.is_empty() {
            return Err(CommandError {
                reason: InvalidCommandReason::Empty,
                list_at: None,
            });
        }

        let mut out = Vec::with_capacity(commands.len());

        for (index, c) in commands.iter().enumerate() {
            let mut c = validate_single_command(c)
                .map_err(|mut e| {
                    e.list_at = Some(index);
                    e
                })?
                .to_owned();

            canonicalize_command(&mut c);

            if c.starts_with("command_list_") {
                return Err(CommandError {
                    reason: InvalidCommandReason::NestedCommandList,
                    list_at: Some(index),
                });
            } else {
                out.push(c.to_owned());
            }
        }

        Ok(Self { commands: out })
    }
}

/// Escape a single argument, prefixing necessary characters with backslashes
fn escape_single_argument(buf: &mut String, argument: &str) {
    for c in argument.chars() {
        if should_escape(c) {
            buf.push('\\');
        }

        buf.push(c);
    }
}

/// If the given character needs to be escaped
fn should_escape(c: char) -> bool {
    c == '\\' || c == '"' || c == '\''
}

/// Validate that a single command string is well-formed
fn validate_single_command(command: &str) -> Result<&str, CommandError> {
    if command.is_empty() {
        return Err(CommandError {
            reason: InvalidCommandReason::Empty,
            list_at: None,
        });
    }

    // If either the first or last character are whitespace we have leading or trailing whitespace
    if command.chars().nth(0).unwrap().is_ascii_whitespace()
        || command.chars().last().unwrap().is_ascii_whitespace()
    {
        return Err(CommandError {
            reason: InvalidCommandReason::UnncessaryWhitespace,
            list_at: None,
        });
    }

    let mut in_command_part = true;
    if let Some((index, c)) = command.char_indices().find(|(index, c)| {
        if in_command_part {
            if is_valid_command_char(*c) {
                false
            } else {
                // The "command" part of the command string is terminated with a space
                if *index != 0 && *c == ' ' {
                    in_command_part = false;
                    false
                } else {
                    true
                }
            }
        } else {
            // The argument part of a command can contain anything except a newline
            *c == '\n'
        }
    }) {
        return Err(CommandError {
            reason: InvalidCommandReason::InvalidCharacter(index, c),
            list_at: None,
        });
    }

    Ok(command)
}

/// Canonicalize (lowercase) the leading command section of the command string
fn canonicalize_command(command: &mut str) {
    let command_end = command
        .char_indices()
        .find(|(_i, c)| !is_valid_command_char(*c))
        .map(|(i, _)| i)
        .unwrap_or(command.len() - 1);

    command[..command_end].make_ascii_lowercase();
}

/// Commands can consist of alphabetic chars and underscores
fn is_valid_command_char(c: char) -> bool {
    c.is_ascii_alphabetic() || c == '_'
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
            InvalidCommandReason::NestedCommandList => write!(
                f,
                "Command attempted to open or close a command list while already in one"
            ),
        }?;

        if let Some(i) = self.list_at {
            write!(f, " (at command list index {})", i)?;
        }

        Ok(())
    }
}
