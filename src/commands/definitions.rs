//! Definitions of commands.

use mpd_protocol::Command as RawCommand;

use std::borrow::Cow;
use std::cmp::min;
use std::ops::{Bound, RangeBounds};
use std::time::Duration;

use super::{
    responses::{self as res, SingleMode},
    Command,
};
use crate::commands::{SongId, SongPosition};

macro_rules! argless_command {
    // Utility branch to generate struct with doc expression
    (#[doc = $doc:expr],
     $item:item) => {
        #[doc = $doc]
        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        $item
    };
    ($name:ident, $command:literal, $response:ty) => {
        argless_command!(
            #[doc = concat!("`", $command, "` command")],
            pub struct $name;
        );

        impl Command for $name {
            type Response = $response;

            fn to_command(self) -> RawCommand {
                RawCommand::new($command)
            }
        }
    };
}

macro_rules! single_arg_command {
    // Utility branch to generate struct with doc expression
    (#[doc = $doc:expr],
     $item:item) => {
        #[doc = $doc]
        #[derive(Clone, Debug, PartialEq, Eq)]
        #[allow(missing_copy_implementations)]
        $item
    };
    ($name:ident, $argtype:ty, $command:literal, $response:ty) => {
        single_arg_command!(
            #[doc = concat!("`", $command, "` command")],
            pub struct $name(pub $argtype);
        );

        impl Command for $name {
            type Response = $response;

            fn to_command(self) -> RawCommand {
                RawCommand::new($command)
                    .argument(self.0.render())
            }
        }
    };
}

macro_rules! impl_display_argument {
    ($($type:ty),+) => {
        $(
            impl Argument for $type {
                fn render(self) -> Cow<'static, str> {
                    Cow::Owned(self.to_string())
                }
            }
        )+
    };
}

trait Argument {
    fn render(self) -> Cow<'static, str>;
}

impl_display_argument!(u8);

impl Argument for bool {
    fn render(self) -> Cow<'static, str> {
        Cow::Borrowed(if self { "1" } else { "0" })
    }
}

argless_command!(Next, "next", res::Empty);
argless_command!(Previous, "previous", res::Empty);
argless_command!(Stop, "stop", res::Empty);
argless_command!(ClearQueue, "clear", res::Empty);

argless_command!(Status, "status", res::Status);
argless_command!(Stats, "stats", res::Stats);

argless_command!(Queue, "playlistinfo", Vec<res::SongInQueue>);
argless_command!(CurrentSong, "currentsong", Option<res::SongInQueue>);

single_arg_command!(SetRandom, bool, "random", res::Empty);
single_arg_command!(SetConsume, bool, "consume", res::Empty);
single_arg_command!(SetRepeat, bool, "repeat", res::Empty);
single_arg_command!(SetPause, bool, "pause", res::Empty);

/// `crossfade` command.
///
/// The given duration is truncated to the seconds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Crossfade(pub Duration);

impl Command for Crossfade {
    type Response = res::Empty;

    fn to_command(self) -> RawCommand {
        let seconds = self.0.as_secs();
        RawCommand::new("crossfade").argument(seconds.to_string())
    }
}

/// `setvol` command.
///
/// Set the volume. The value is truncated to fit in the range `0..=100`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SetVolume(pub u8);

impl Command for SetVolume {
    type Response = res::Empty;

    fn to_command(self) -> RawCommand {
        let volume = min(self.0, 100);
        RawCommand::new("setvol").argument(volume.to_string())
    }
}

/// `single` command.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SetSingle(pub SingleMode);

impl Command for SetSingle {
    type Response = res::Empty;

    fn to_command(self) -> RawCommand {
        let single = match self.0 {
            SingleMode::Disabled => "0",
            SingleMode::Enabled => "1",
            SingleMode::Oneshot => "oneshot",
        };

        RawCommand::new("single").argument(single)
    }
}

/// Modes to target a song with a command.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Song {
    /// By ID
    Id(SongId),
    /// By position in the queue.
    Position(SongPosition),
}

impl From<SongId> for Song {
    fn from(id: SongId) -> Self {
        Self::Id(id)
    }
}

impl From<SongPosition> for Song {
    fn from(pos: SongPosition) -> Self {
        Self::Position(pos)
    }
}

/// `seek` and `seekid` commands.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SeekTo(pub Song, pub Duration);

impl Command for SeekTo {
    type Response = res::Empty;

    fn to_command(self) -> RawCommand {
        let (command, song_arg) = match self.0 {
            Song::Id(id) => ("seekid", id.0.to_string()),
            Song::Position(pos) => ("seek", pos.0.to_string()),
        };

        RawCommand::new(command)
            .argument(song_arg)
            .argument(format!("{:.3}", self.1.as_secs_f64()))
    }
}

/// Possible ways to seek in the current song.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SeekMode {
    /// Forwards from current position.
    Forward(Duration),
    /// Backwards from current position.
    Backward(Duration),
    /// To the absolute position in the current song.
    Absolute(Duration),
}

/// `seekcur` command.
///
/// Seek in the current song.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Seek(pub SeekMode);

impl Command for Seek {
    type Response = res::Empty;

    fn to_command(self) -> RawCommand {
        let time = match self.0 {
            SeekMode::Absolute(pos) => format!("{:.3}", pos.as_secs_f64()),
            SeekMode::Forward(time) => format!("+{:.3}", time.as_secs_f64()),
            SeekMode::Backward(time) => format!("-{:.3}", time.as_secs_f64()),
        };

        RawCommand::new("seekcur").argument(time)
    }
}

/// `play` and `playid` commands.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Play(Option<Song>);

impl Play {
    /// Play the current song (when paused or stopped).
    pub fn current() -> Self {
        Self(None)
    }

    /// Play the given song.
    pub fn song<S>(song: S) -> Self
    where
        S: Into<Song>,
    {
        Self(Some(song.into()))
    }
}

impl Command for Play {
    type Response = res::Empty;

    fn to_command(self) -> RawCommand {
        match self.0 {
            None => RawCommand::new("play"),
            Some(song) => {
                let (command, arg) = match song {
                    Song::Position(pos) => ("play", pos.0.to_string()),
                    Song::Id(id) => ("playid", id.0.to_string()),
                };

                RawCommand::new(command).argument(arg)
            }
        }
    }
}

/// `addid` command.
///
/// Add a song to the queue, returning its ID. If [`at`] is not used, the song will be appended to
/// the queue.
///
/// [`at`]: #method.at
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Add {
    uri: String,
    position: Option<usize>,
}

impl Add {
    /// Add the song with the given URI.
    ///
    /// Only individual files are supported.
    pub fn uri(uri: String) -> Self {
        Self {
            uri,
            position: None,
        }
    }

    /// Add the URI at the given position in the queue.
    pub fn at(mut self, position: usize) -> Self {
        self.position = Some(position);
        self
    }
}

impl Command for Add {
    type Response = SongId;

    fn to_command(self) -> RawCommand {
        let mut command = RawCommand::new("addid").argument(self.uri);

        if let Some(pos) = self.position {
            command.add_argument(pos.to_string()).unwrap();
        }

        command
    }
}

/// `delete` and `deleteid` commands.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Delete(Target);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Target {
    Id(SongId),
    Range(usize, Option<usize>),
}

impl Delete {
    /// Remove the given ID from the queue.
    pub fn id(id: SongId) -> Self {
        Self(Target::Id(id))
    }

    /// Remove the song at the given position from the queue.
    pub fn position(pos: SongPosition) -> Self {
        Self(Target::Range(pos.0, Some(pos.0 + 1)))
    }

    /// Remove the given range from the queue.
    ///
    /// The range must have at least a lower bound.
    pub fn range<R>(range: R) -> Self
    where
        R: RangeBounds<SongPosition>,
    {
        Self(Target::range(range))
    }
}

impl Target {
    fn range<R: RangeBounds<SongPosition>>(range: R) -> Self {
        let lower = match range.start_bound() {
            Bound::Included(pos) => pos.0,
            _ => panic!("range must have a lower bound"),
        };

        let upper = match range.end_bound() {
            Bound::Excluded(pos) => Some(pos.0),
            Bound::Included(pos) => Some(pos.0 + 1),
            Bound::Unbounded => None,
        };

        Self::Range(lower, upper)
    }
}

impl Command for Delete {
    type Response = res::Empty;

    fn to_command(self) -> RawCommand {
        match self.0 {
            Target::Id(id) => RawCommand::new("deleteid").argument(id.0.to_string()),
            Target::Range(from, up_to) => {
                let range = match up_to {
                    Some(end) => format!("{}:{}", from, end),
                    None => format!("{}:", from),
                };

                RawCommand::new("delete").argument(range)
            }
        }
    }
}

/// `move` and `moveid` commands.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Move {
    from: Target,
    to: usize,
}

impl Move {
    /// Move the song with the given ID to the given position.
    pub fn id(id: SongId, to: SongPosition) -> Self {
        Self {
            from: Target::Id(id),
            to: to.0,
        }
    }

    /// Move the song at the given position to the given position.
    pub fn position(from: SongPosition, to: SongPosition) -> Self {
        Self {
            from: Target::Range(from.0, Some(from.0 + 1)),
            to: to.0,
        }
    }

    /// Move the given range of song positions to the given position.
    pub fn range<R>(range: R, to: SongPosition) -> Self
    where
        R: RangeBounds<SongPosition>,
    {
        Self {
            from: Target::range(range),
            to: to.0,
        }
    }
}

impl Command for Move {
    type Response = res::Empty;

    fn to_command(self) -> RawCommand {
        match self.from {
            Target::Id(id) => RawCommand::new("moveid")
                .argument(id.0.to_string())
                .argument(self.to.to_string()),
            Target::Range(lower, upper) => RawCommand::new("move")
                .argument(match upper {
                    Some(upper) => format!("{}:{}", lower, upper),
                    None => format!("{}:", lower),
                })
                .argument(self.to.to_string()),
        }
    }
}
