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
pub struct Command(CommandType);

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
enum CommandType {
    Single(String),
    List(Vec<String>),
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
    /// The was empty.
    Empty,
    /// The command string
    InvalidCharacter(usize, char),
    /// The contained trailing or leading whitespace (whitespace in the middle of commands is used to separate arguments).
    UnncessaryWhitespace,
    /// Attempted to start a nested command list, which are not supported.
    NestedCommandList,
}

impl Command {
    /// Create a new command, but panic instead of returning a `Result` when the conversion fails.
    ///
    /// This may be useful in cases where you supply known-good commands for simplicity.
    pub fn new<C>(c: C) -> Self
    where
        C: TryInto<Self>,
        <C as TryInto<Self>>::Error: Debug,
    {
        c.try_into().expect("invalid command")
    }

    /// Render the command to the wire representation. Commands are automatically wrapped in
    /// command lists if necessary.
    pub fn render(self) -> String {
        match self.0 {
            CommandType::Single(mut c) => {
                c.push('\n');
                c
            }
            CommandType::List(c) => {
                let mut out = String::with_capacity(
                    COMMAND_LIST_BEGIN.len()
                        + COMMAND_LIST_END.len()
                        + c.iter().fold(0, |acc, c| acc + c.len() + 1),
                );

                out.push_str(COMMAND_LIST_BEGIN);

                for command in c {
                    // Each command is simply followed by a newline
                    out.push_str(command.as_str());
                    out.push('\n');
                }

                out.push_str(COMMAND_LIST_END);

                out
            }
        }
    }
}

impl TryFrom<&str> for Command {
    type Error = CommandError;

    fn try_from(c: &str) -> Result<Self, Self::Error> {
        let cs = c.split('\n').collect::<Vec<_>>();

        if cs.is_empty() {
            Err(CommandError {
                reason: InvalidCommandReason::Empty,
                list_at: None,
            })
        } else if cs.len() == 1 {
            let mut c = validate_single_command(cs.first().unwrap())?.to_owned();
            canonicalize_command(&mut c);
            Ok(Self(CommandType::Single(c)))
        } else {
            cs.iter().enumerate().try_for_each(|(index, c)| {
                if c.is_empty() {
                    Err(CommandError {
                        reason: InvalidCommandReason::Empty,
                        list_at: Some(index),
                    })
                } else {
                    validate_single_command(c).map_err(|mut e| {
                        e.list_at = Some(index);
                        e
                    })?;

                    Ok(())
                }
            })?;

            let mut cs = cs.into_iter().map(|s| s.to_owned()).collect::<Vec<_>>();

            cs.iter_mut().enumerate().try_for_each(|(i, c)| {
                canonicalize_command(c);

                if c.starts_with("command_list_") {
                    Err(CommandError {
                        reason: InvalidCommandReason::NestedCommandList,
                        list_at: Some(i),
                    })
                } else {
                    Ok(())
                }
            })?;

            Ok(Self(CommandType::List(cs)))
        }
    }
}

fn validate_single_command(command: &str) -> Result<&str, CommandError> {
    if command.is_empty() {
        return Err(CommandError {
            reason: InvalidCommandReason::Empty,
            list_at: None,
        });
    }

    // If either the first or last character are whitespace we have leading or trailing whitespace
    if command.chars().nth(0).unwrap().is_whitespace()
        || command.chars().last().unwrap().is_whitespace()
    {
        return Err(CommandError {
            reason: InvalidCommandReason::UnncessaryWhitespace,
            list_at: None,
        });
    }

    if let Some((index, c)) = command
        .char_indices()
        .find(|(_, c)| !(is_valid_command_char(*c) || c.is_whitespace()))
    {
        return Err(CommandError {
            reason: InvalidCommandReason::InvalidCharacter(index, c),
            list_at: None,
        });
    }

    Ok(command)
}

fn canonicalize_command(command: &mut str) {
    let command_end = command
        .char_indices()
        .find(|(_i, c)| !is_valid_command_char(*c))
        .map(|(i, _)| i)
        .unwrap_or(command.len() - 1);

    command[..command_end].make_ascii_lowercase();
}

fn is_valid_command_char(c: char) -> bool {
    c.is_alphabetic() || c == '_'
}

impl Error for CommandError {}

impl fmt::Display for CommandError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.reason {
            InvalidCommandReason::Empty => write!(f, "Command was empty"),
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
                "Command attempted to open a command list while already in one"
            ),
        }?;

        if let Some(i) = self.list_at {
            write!(f, " (at command list index {})", i)?;
        }

        Ok(())
    }
}
