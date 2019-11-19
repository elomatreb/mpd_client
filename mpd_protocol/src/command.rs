//! Commands we can send to MPD. See the [MPD protocol command
//! reference](https://www.musicpd.org/doc/html/protocol.html#command-reference) for further
//! details on each individual command.

mod argument;

use std::borrow::Cow;
use std::time::Duration;

use argument::Argument;

/// A command
pub trait Command {
    /// Render the command to the wire representation
    fn render(self) -> String;

    /// Prepare Command for transmission
    fn into_command(self) -> Box<dyn Command>
    where
        Self: Sized + 'static,
    {
        Box::new(self)
    }
}

/// Type for song IDs, used to identify songs in the play queue.
pub type SongId = u64;

macro_rules! impl_argless_command {
    ($type:ty, $command:expr) => {
        impl Command for $type {
            fn render(self) -> String {
                String::from($command)
            }
        }
    };
}

macro_rules! impl_single_arg_command {
    ($type:ty, $command:expr) => {
        impl Command for $type {
            fn render(self) -> String {
                format!(concat!($command, " {}"), self.0.render())
            }
        }
    };
}

/// Clear current errors. Corresponds to the `clearerror` command.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ClearError;
impl_argless_command!(ClearError, "clearerror");

/// Get the details of the currently playing song. Corresponds to the `currentsong` command.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct GetCurrentSong;
impl_argless_command!(GetCurrentSong, "currentsong");

/// Idle, waiting for changes in the given [subsystems](enum.Subsystem.html). An empty `Vec` or
/// [`Subsystem::ALL`](enum.Subsystem.html#associatedconstant.ALL) means all subsystems.
///
/// While idling, connection timeout is disabled, and [`CancelIdle`](struct.CancelIdle.html) is
/// the only allowed command.
///
/// Corresponds to the `idle` command.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Idle(pub Vec<Subsystem>);
impl_single_arg_command!(Idle, "idle");

/// The MPD subsystems which the [`Idle`](struct.Idle.html) command can wait for changes in. See the
/// [command documentation](https://www.musicpd.org/doc/html/protocol.html#command-idle) for details
/// of the subsystems.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Subsystem {
    Database,
    Update,
    StoredPlaylist,
    Playlist,
    Player,
    Mixer,
    Output,
    Options,
    Partition,
    Sticker,
    Subscription,
    Message,
}

impl Subsystem {
    /// Utility alias for waiting for all subsystems
    pub const ALL: Vec<Self> = Vec::new();
}

impl Argument for Subsystem {
    fn render(self) -> Cow<'static, str> {
        let s = match self {
            Subsystem::Database => "database",
            Subsystem::Update => "update",
            Subsystem::StoredPlaylist => "stored_playlist",
            Subsystem::Playlist => "playlist",
            Subsystem::Player => "player",
            Subsystem::Mixer => "mixer",
            Subsystem::Output => "output",
            Subsystem::Options => "options",
            Subsystem::Partition => "partition",
            Subsystem::Sticker => "sticker",
            Subsystem::Subscription => "subscriptions",
            Subsystem::Message => "message",
        };

        Cow::Borrowed(s)
    }
}

/// Cancel an [`Idle`](struct.Idle.html). Only command allowed during Idle, and only allowed during
/// Idle.
///
/// Corresponds to the `noidle` command.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct CancelIdle;
impl_argless_command!(CancelIdle, "noidle");

/// Get player status. Corresponds to the `status` command.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct GetStatus;
impl_argless_command!(GetStatus, "status");

/// Get server statistics. Corresponds to the `stats` command.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct GetStatistics;
impl_argless_command!(GetStatistics, "stats");

/// Enable or disable "consume" mode. `true` means enabled.
///
/// Corresponds to the `consume` command.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SetConsume(pub bool);
impl_single_arg_command!(SetConsume, "consume");

/// Set crossfade duration. Rounded down to the nearest full second.
///
/// Corresponds to the `crossfade` command.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SetCrossfade(pub Duration);

impl Command for SetCrossfade {
    fn render(self) -> String {
        format!("crossfade {}", self.0.as_secs())
    }
}

/// Set MixRamp threshold.
///
/// In decibels. Corresponds to the `mixrampdb` command.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SetMixRampDb(pub f32);
impl_single_arg_command!(SetMixRampDb, "mixrampdb");

/// Set delay for MixRamp. Corresponds to the `mixrampdelay` command.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SetMixRampDelay(pub Duration);
impl_single_arg_command!(SetMixRampDelay, "mixrampdelay");

/// Set "random" state. `true` means enabled. Corresponds to the `random` command.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SetRandom(pub bool);
impl_single_arg_command!(SetRandom, "random");

/// Set "repeat" state. `true` means enabled. Corresponds to the `repeat` command.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SetRepeat(pub bool);
impl_single_arg_command!(SetRepeat, "repeat");

/// Set volume. Range is 0-100, values larger than 100 are truncated to 100.
///
/// Corresponds to the `setvol` command.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SetVolume(pub u8);

impl Command for SetVolume {
    fn render(self) -> String {
        format!("setvol {}", self.0.min(100))
    }
}

/// Set "single" state. Corresponds to the `single` command.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SetSingle(pub Single);
impl_single_arg_command!(SetSingle, "single");

/// Possible modes for the "single" mode
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Single {
    Disabled,
    Enabled,
    OneShot,
}

impl Argument for Single {
    fn render(self) -> Cow<'static, str> {
        Cow::Borrowed(match self {
            Single::Disabled => "0",
            Single::Enabled => "1",
            Single::OneShot => "oneshot",
        })
    }
}

/// Set ReplayGain mode. Corresponds to the `replay_gain_mode` command.
pub struct SetReplayGainMode(pub ReplayGain);
impl_single_arg_command!(SetReplayGainMode, "replay_gain_mode");

/// Possible ReplayGain modes
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ReplayGain {
    Off,
    Track,
    Album,
    Auto,
}

impl Argument for ReplayGain {
    fn render(self) -> Cow<'static, str> {
        Cow::Borrowed(match self {
            ReplayGain::Off => "off",
            ReplayGain::Track => "track",
            ReplayGain::Album => "album",
            ReplayGain::Auto => "auto",
        })
    }
}

/// Get the ReplayGain status (mode). Corresponds to the `replay_gain_status` command.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct GetReplayGainStatus;
impl_argless_command!(GetReplayGainStatus, "replay_gain_status");

/// Play next song in queue. Corresponds to `next` command.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Next;
impl_argless_command!(Next, "next");

/// Set pause state. `true` means playback is paused. Corresponds to `pause` command.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Pause(pub bool);
impl_single_arg_command!(Pause, "pause");

/// Play the song with the given ID. Corresponds to `playid` command.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PlayId(pub SongId);
impl_single_arg_command!(PlayId, "playid");

/// Play previous song in queue. Corresponds to `previous` command.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Previous;
impl_argless_command!(Previous, "previous");

/// Seek to the given position in the track with the given ID.
///
/// Corresponds to `seekid` command.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SeekId {
    pub id: SongId,
    pub position: Duration,
}

impl Command for SeekId {
    fn render(self) -> String {
        format!("seekid {} {}", self.id.render(), self.position.render())
    }
}

/// Seek in the currently playing track. Corresponds to the `seekcur` command.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SeekCurrent(pub Seek);
impl_single_arg_command!(SeekCurrent, "seekcur");

/// Possible seek modes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Seek {
    /// Seek to the absolute given position
    Absolute(Duration),
    /// Seek backwards relative to the current play position
    Backwards(Duration),
    /// Seek forwards relative to the current play position
    Forwards(Duration),
}

impl Argument for Seek {
    fn render(self) -> Cow<'static, str> {
        match self {
            Seek::Absolute(pos) => pos.render(),
            Seek::Backwards(pos) => Cow::Owned(format!("-{}", pos.render())),
            Seek::Forwards(pos) => Cow::Owned(format!("+{}", pos.render())),
        }
    }
}

/// Stop playback. Corresponds to the `stop` command.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Stop;
impl_argless_command!(Stop, "stop");
