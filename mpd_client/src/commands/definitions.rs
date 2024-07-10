//! Definitions of commands.

use std::{
    cmp::min,
    fmt::Write,
    ops::{Bound, RangeBounds},
    time::Duration,
};

use bytes::BytesMut;
use mpd_protocol::{
    command::{Argument, Command as RawCommand},
    response::Frame,
};

use crate::{
    commands::{Command, ReplayGainMode, SeekMode, SingleMode, Song, SongId, SongPosition},
    filter::Filter,
    responses::{self as res, value, TypedResponseError},
    tag::Tag,
};

macro_rules! argless_command {
    // Utility branch to generate struct with doc expression
    (#[doc = $doc:expr],
     $item:item) => {
        #[doc = $doc]
        #[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
        $item
    };
    ($name:ident, $command:literal) => {
        argless_command!(
            #[doc = concat!("`", $command, "` command.")],
            pub struct $name;
        );

        impl Command for $name {
            type Response = ();

            fn command(&self) -> RawCommand {
                RawCommand::new($command)
            }

            fn response(self, _: Frame) -> Result<Self::Response, TypedResponseError> {
                Ok(())
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
    ($name:ident $(<$lt:lifetime>)?, $argtype:ty, $command:literal) => {
        single_arg_command!(
            #[doc = concat!("`", $command, "` command.")],
            pub struct $name $(<$lt>)? (pub $argtype);
        );

        impl $(<$lt>)? Command for $name $(<$lt>)? {
            type Response = ();

            fn command(&self) -> RawCommand {
                RawCommand::new($command)
                    .argument(&self.0)
            }

            fn response(self, _: Frame) -> Result<Self::Response, TypedResponseError> {
                Ok(())
            }
        }
    };
}

argless_command!(ClearQueue, "clear");
argless_command!(Next, "next");
argless_command!(Ping, "ping");
argless_command!(Previous, "previous");
argless_command!(Stop, "stop");

single_arg_command!(EnableOutput<'a>, &'a str, "enableoutput");
single_arg_command!(MoveOutput<'a>, &'a str, "moveoutput");
single_arg_command!(NewPartition<'a>, &'a str, "newpartition");
single_arg_command!(SwitchPartition<'a>, &'a str, "partition");
single_arg_command!(SetVol<'a>, &'a str, "setvol");

single_arg_command!(ClearPlaylist<'a>, &'a str, "playlistclear");
single_arg_command!(DeletePlaylist<'a>, &'a str, "rm");
single_arg_command!(SaveQueueAsPlaylist<'a>, &'a str, "save");
single_arg_command!(SetConsume, bool, "consume");
single_arg_command!(SetPause, bool, "pause");
single_arg_command!(SetRandom, bool, "random");
single_arg_command!(SetRepeat, bool, "repeat");
single_arg_command!(SubscribeToChannel<'a>, &'a str, "subscribe");
single_arg_command!(UnsubscribeFromChannel<'a>, &'a str, "unsubscribe");

/// `replay_gain_status` command.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ReplayGainStatus;

impl Command for ReplayGainStatus {
    type Response = res::ReplayGainStatus;

    fn command(&self) -> RawCommand {
        RawCommand::new("replay_gain_status")
    }

    fn response(self, frame: Frame) -> Result<Self::Response, TypedResponseError> {
        res::ReplayGainStatus::from_frame(frame)
    }
}

/// `status` command.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ListPartitions;

impl Command for ListPartitions {
    type Response = Vec<res::Partition>;

    fn command(&self) -> RawCommand {
        RawCommand::new("listpartitions")
    }

    fn response(self, frame: Frame) -> Result<Self::Response, TypedResponseError> {
        res::Partition::from_frame_multi(frame)
    }
}

/// `status` command.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Status;

impl Command for Status {
    type Response = res::Status;

    fn command(&self) -> RawCommand {
        RawCommand::new("status")
    }

    fn response(self, frame: Frame) -> Result<Self::Response, TypedResponseError> {
        res::Status::from_frame(frame)
    }
}

/// `stats` command.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Stats;

impl Command for Stats {
    type Response = res::Stats;

    fn command(&self) -> RawCommand {
        RawCommand::new("stats")
    }

    fn response(self, frame: Frame) -> Result<Self::Response, TypedResponseError> {
        res::Stats::from_frame(frame)
    }
}

/// `playlistinfo` command.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Queue;

impl Command for Queue {
    type Response = Vec<res::SongInQueue>;

    fn command(&self) -> RawCommand {
        RawCommand::new("playlistinfo")
    }

    fn response(self, frame: Frame) -> Result<Self::Response, TypedResponseError> {
        res::SongInQueue::from_frame_multi(frame)
    }
}

impl Queue {
    /// Get the metadata about the entire queue.
    pub fn all() -> Queue {
        Queue
    }

    /// Get the metadata for a specific song in the queue.
    pub fn song<S>(song: S) -> QueueRange
    where
        S: Into<Song>,
    {
        QueueRange(SongOrSongRange::Single(song.into()))
    }

    /// Get the metadata for a range of songs in the queue.
    pub fn range<R>(range: R) -> QueueRange
    where
        R: RangeBounds<SongPosition>,
    {
        QueueRange(SongOrSongRange::Range(SongRange::new(range)))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct SongRange {
    from: usize,
    to: Option<usize>,
}

impl SongRange {
    fn new_usize<R: RangeBounds<usize>>(range: R) -> Self {
        let from = match range.start_bound() {
            Bound::Excluded(pos) => pos.saturating_add(1),
            Bound::Included(pos) => *pos,
            Bound::Unbounded => 0,
        };

        let to = match range.end_bound() {
            Bound::Excluded(pos) => Some(*pos),
            Bound::Included(pos) => Some(pos.saturating_add(1)),
            Bound::Unbounded => None,
        };

        Self { from, to }
    }

    fn new<R: RangeBounds<SongPosition>>(range: R) -> Self {
        let from = match range.start_bound() {
            Bound::Excluded(pos) => Bound::Excluded(pos.0),
            Bound::Included(pos) => Bound::Included(pos.0),
            Bound::Unbounded => Bound::Unbounded,
        };

        let to = match range.end_bound() {
            Bound::Excluded(pos) => Bound::Excluded(pos.0),
            Bound::Included(pos) => Bound::Included(pos.0),
            Bound::Unbounded => Bound::Unbounded,
        };

        Self::new_usize((from, to))
    }
}

impl Argument for SongRange {
    fn render(&self, buf: &mut BytesMut) {
        if let Some(to) = self.to {
            write!(buf, "{}:{}", self.from, to).unwrap();
        } else {
            write!(buf, "{}:", self.from).unwrap();
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SongOrSongRange {
    /// Single Song
    Single(Song),

    /// Song Range
    Range(SongRange),
}

/// `playlistinfo` / `playlistid` commands.
///
/// These return the metadata of specific individual songs or subranges of the queue.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct QueueRange(SongOrSongRange);

impl QueueRange {
    /// Get the metadata for a specific song in the queue.
    pub fn song<S>(song: S) -> Self
    where
        S: Into<Song>,
    {
        Self(SongOrSongRange::Single(song.into()))
    }

    /// Get the metadata for a range of songs in the queue.
    pub fn range<R>(range: R) -> Self
    where
        R: RangeBounds<SongPosition>,
    {
        Self(SongOrSongRange::Range(SongRange::new(range)))
    }
}

impl Command for QueueRange {
    type Response = Vec<res::SongInQueue>;

    fn command(&self) -> RawCommand {
        match self.0 {
            SongOrSongRange::Single(Song::Id(id)) => RawCommand::new("playlistid").argument(id),
            SongOrSongRange::Single(Song::Position(pos)) => {
                RawCommand::new("playlistinfo").argument(pos)
            }
            SongOrSongRange::Range(range) => RawCommand::new("playlistinfo").argument(range),
        }
    }

    fn response(self, frame: Frame) -> Result<Self::Response, TypedResponseError> {
        res::SongInQueue::from_frame_multi(frame)
    }
}

/// `currentsong` command.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CurrentSong;

impl Command for CurrentSong {
    type Response = Option<res::SongInQueue>;

    fn command(&self) -> RawCommand {
        RawCommand::new("currentsong")
    }

    fn response(self, frame: Frame) -> Result<Self::Response, TypedResponseError> {
        res::SongInQueue::from_frame_single(frame)
    }
}

/// `listplaylists` command.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GetPlaylists;

impl Command for GetPlaylists {
    type Response = Vec<res::Playlist>;

    fn command(&self) -> RawCommand {
        RawCommand::new("listplaylists")
    }

    fn response(self, frame: Frame) -> Result<Self::Response, TypedResponseError> {
        res::Playlist::parse_frame(frame)
    }
}

/// `tagtypes` command.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GetEnabledTagTypes;

impl Command for GetEnabledTagTypes {
    type Response = Vec<Tag>;

    fn command(&self) -> RawCommand {
        RawCommand::new("tagtypes")
    }

    fn response(self, frame: Frame) -> Result<Self::Response, TypedResponseError> {
        let mut out = Vec::with_capacity(frame.fields_len());
        for (key, value) in frame {
            if &*key != "tagtype" {
                return Err(TypedResponseError::unexpected_field(
                    "tagtype",
                    key.as_ref(),
                ));
            }

            let tag = Tag::try_from(&*value)
                .map_err(|e| TypedResponseError::invalid_value("tagtype", value).source(e))?;

            out.push(tag);
        }

        Ok(out)
    }
}

/// `listplaylistinfo` command.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GetPlaylist<'a>(pub &'a str);

impl<'a> Command for GetPlaylist<'a> {
    type Response = Vec<res::Song>;

    fn command(&self) -> RawCommand {
        RawCommand::new("listplaylistinfo").argument(self.0)
    }

    fn response(self, frame: Frame) -> Result<Self::Response, TypedResponseError> {
        res::Song::from_frame_multi(frame)
    }
}

/// `setvol` command.
///
/// Set the volume. The value is truncated to fit in the range `0..=100`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SetVolume(pub u8);

impl Command for SetVolume {
    type Response = ();

    fn command(&self) -> RawCommand {
        let volume = min(self.0, 100);
        RawCommand::new("setvol").argument(volume)
    }

    fn response(self, _: Frame) -> Result<Self::Response, TypedResponseError> {
        Ok(())
    }
}

/// `getvol` command.
/// Read the volume. If there is no mixer, MPD will emit an empty response.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GetVolume {
}

impl GetVolume {
    /// read the volume
    pub fn new() -> Self {
        Self {}
    }
}

impl Command for GetVolume {
    type Response = Option<String>;

    fn command(&self) -> RawCommand {
        RawCommand::new("getvol")
    }

    fn response(self, mut frame: Frame) -> Result<Self::Response, TypedResponseError> {
        match frame.get("volume") {
            None => {Ok(None)}
            Some(volume) => {
                Ok(Some(volume))
            }
        }
    }
}

/// `single` command.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SetSingle(pub SingleMode);

impl Command for SetSingle {
    type Response = ();

    fn command(&self) -> RawCommand {
        let single = match self.0 {
            SingleMode::Disabled => "0",
            SingleMode::Enabled => "1",
            SingleMode::Oneshot => "oneshot",
        };

        RawCommand::new("single").argument(single)
    }

    fn response(self, _: Frame) -> Result<Self::Response, TypedResponseError> {
        Ok(())
    }
}

/// 'replay_gain_mode' command
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SetReplayGainMode(pub ReplayGainMode);

impl Command for SetReplayGainMode {
    type Response = ();

    fn command(&self) -> RawCommand {
        let rgm = match self.0 {
            ReplayGainMode::Off => "off",
            ReplayGainMode::Track => "track",
            ReplayGainMode::Album => "album",
            ReplayGainMode::Auto => "auto",
        };

        RawCommand::new("replay_gain_mode").argument(rgm)
    }

    fn response(self, _: Frame) -> Result<Self::Response, TypedResponseError> {
        Ok(())
    }
}

/// `crossfade` command.
///
/// The given duration is rounded down to whole seconds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Crossfade(pub Duration);

impl Command for Crossfade {
    type Response = ();

    fn command(&self) -> RawCommand {
        let seconds = self.0.as_secs();
        RawCommand::new("crossfade").argument(seconds)
    }

    fn response(self, _: Frame) -> Result<Self::Response, TypedResponseError> {
        Ok(())
    }
}

/// `seek` and `seekid` commands.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SeekTo(pub Song, pub Duration);

impl Command for SeekTo {
    type Response = ();

    fn command(&self) -> RawCommand {
        let command = match self.0 {
            Song::Position(pos) => RawCommand::new("seek").argument(pos),
            Song::Id(id) => RawCommand::new("seekid").argument(id),
        };

        command.argument(self.1)
    }

    fn response(self, _: Frame) -> Result<Self::Response, TypedResponseError> {
        Ok(())
    }
}

/// `seekcur` command.
///
/// Seek in the current song.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Seek(pub SeekMode);

impl Command for Seek {
    type Response = ();

    fn command(&self) -> RawCommand {
        let time = match self.0 {
            SeekMode::Absolute(pos) => format!("{:.3}", pos.as_secs_f64()),
            SeekMode::Forward(time) => format!("+{:.3}", time.as_secs_f64()),
            SeekMode::Backward(time) => format!("-{:.3}", time.as_secs_f64()),
        };

        RawCommand::new("seekcur").argument(time)
    }

    fn response(self, _: Frame) -> Result<Self::Response, TypedResponseError> {
        Ok(())
    }
}

/// `shuffle` command
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Shuffle(Option<SongRange>);

impl Shuffle {
    /// Shuffle entire queue
    pub fn all() -> Self {
        Self(None)
    }

    /// Shuffle a range of songs
    ///
    /// The range must have at least a lower bound.
    pub fn range<R>(range: R) -> Self
    where
        R: RangeBounds<SongPosition>,
    {
        Self(Some(SongRange::new(range)))
    }
}

impl Command for Shuffle {
    type Response = ();

    fn command(&self) -> RawCommand {
        match self.0 {
            None => RawCommand::new("shuffle"),
            Some(range) => RawCommand::new("shuffle").argument(range),
        }
    }

    fn response(self, _: Frame) -> Result<Self::Response, TypedResponseError> {
        Ok(())
    }
}

/// `play` and `playid` commands.
#[derive(Clone, Debug, PartialEq, Eq)]
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
    type Response = ();

    fn command(&self) -> RawCommand {
        match self.0 {
            None => RawCommand::new("play"),
            Some(Song::Position(pos)) => RawCommand::new("play").argument(pos),
            Some(Song::Id(id)) => RawCommand::new("playid").argument(id),
        }
    }

    fn response(self, _: Frame) -> Result<Self::Response, TypedResponseError> {
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PositionOrRelative {
    Absolute(SongPosition),
    BeforeCurrent(usize),
    AfterCurrent(usize),
}

impl Argument for PositionOrRelative {
    fn render(&self, buf: &mut BytesMut) {
        match self {
            PositionOrRelative::Absolute(pos) => pos.render(buf),
            PositionOrRelative::AfterCurrent(x) => write!(buf, "+{x}").unwrap(),
            PositionOrRelative::BeforeCurrent(x) => write!(buf, "-{x}").unwrap(),
        }
    }
}

/// `addid` command.
///
/// Add a song to the queue, returning its ID. If neither of [`Add::at`], [`Add::before_current`],
/// or [`Add::after_current`] is used, the song will be appended to the queue.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Add<'a> {
    uri: &'a str,
    position: Option<PositionOrRelative>,
}

impl<'a> Add<'a> {
    /// Add the song with the given URI.
    ///
    /// Only individual files are supported.
    pub fn uri(uri: &'a str) -> Self {
        Self {
            uri,
            position: None,
        }
    }

    /// Add the URI at the given position in the queue.
    pub fn at<P: Into<SongPosition>>(mut self, position: P) -> Self {
        self.position = Some(PositionOrRelative::Absolute(position.into()));
        self
    }

    /// Add the URI `delta` positions before the current song.
    ///
    /// A `delta` of 0 is immediately before the current song.
    ///
    /// **NOTE**: Supported on protocol versions later than 0.23.
    pub fn before_current(mut self, delta: usize) -> Self {
        self.position = Some(PositionOrRelative::BeforeCurrent(delta));
        self
    }

    /// Add the URI `delta` positions after the current song.
    ///
    /// A `delta` of 0 is immediately after the current song.
    ///
    /// **NOTE**: Supported on protocol versions later than 0.23.
    pub fn after_current(mut self, delta: usize) -> Self {
        self.position = Some(PositionOrRelative::AfterCurrent(delta));
        self
    }
}

impl<'a> Command for Add<'a> {
    type Response = SongId;

    fn command(&self) -> RawCommand {
        let mut command = RawCommand::new("addid").argument(self.uri);

        if let Some(pos) = self.position {
            command.add_argument(pos).unwrap();
        }

        command
    }

    fn response(self, mut frame: Frame) -> Result<Self::Response, TypedResponseError> {
        value(&mut frame, "Id").map(SongId)
    }
}

/// `delete` and `deleteid` commands.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Delete(Target);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Target {
    Id(SongId),
    Range(SongRange),
}

impl Delete {
    /// Remove the given ID from the queue.
    pub fn id(id: SongId) -> Self {
        Self(Target::Id(id))
    }

    /// Remove the song at the given position from the queue.
    pub fn position(pos: SongPosition) -> Self {
        let range = SongRange::new(pos..=pos);
        Self(Target::Range(range))
    }

    /// Remove the given range from the queue.
    ///
    /// The range must have at least a lower bound.
    pub fn range<R>(range: R) -> Self
    where
        R: RangeBounds<SongPosition>,
    {
        Self(Target::Range(SongRange::new(range)))
    }
}

impl Command for Delete {
    type Response = ();

    fn command(&self) -> RawCommand {
        match self.0 {
            Target::Id(id) => RawCommand::new("deleteid").argument(id),
            Target::Range(range) => RawCommand::new("delete").argument(range),
        }
    }

    fn response(self, _: Frame) -> Result<Self::Response, TypedResponseError> {
        Ok(())
    }
}

/// `move` and `moveid` commands.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Move {
    from: Target,
    to: PositionOrRelative,
}

impl Move {
    /// Move the song with the given ID.
    pub fn id(id: SongId) -> MoveBuilder {
        MoveBuilder(Target::Id(id))
    }

    /// Move the song at the given position.
    pub fn position(position: SongPosition) -> MoveBuilder {
        MoveBuilder(Target::Range(SongRange::new(position..=position)))
    }

    /// Move the given range of song positions.
    ///
    /// # Panics
    ///
    /// The given range must have an end. If a range with an open end is passed, this will panic.
    pub fn range<R>(range: R) -> MoveBuilder
    where
        R: RangeBounds<SongPosition>,
    {
        if let Bound::Unbounded = range.end_bound() {
            panic!("move commands must not have an open end");
        }

        MoveBuilder(Target::Range(SongRange::new(range)))
    }
}

/// Builder for `move` or `moveid` commands.
///
/// Returned by methods on [`Move`].
#[must_use]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MoveBuilder(Target);

impl MoveBuilder {
    /// Move the selection to the given absolute queue position.
    pub fn to_position(self, position: SongPosition) -> Move {
        Move {
            from: self.0,
            to: PositionOrRelative::Absolute(position),
        }
    }

    /// Move the selection to the given `delta` after the current song.
    ///
    /// A `delta` of 0 means immediately after the current song.
    pub fn after_current(self, delta: usize) -> Move {
        Move {
            from: self.0,
            to: PositionOrRelative::AfterCurrent(delta),
        }
    }

    /// Move the selection to the given `delta` before the current song.
    ///
    /// A `delta` of 0 means immediately before the current song.
    pub fn before_current(self, delta: usize) -> Move {
        Move {
            from: self.0,
            to: PositionOrRelative::BeforeCurrent(delta),
        }
    }
}

impl Command for Move {
    type Response = ();

    fn command(&self) -> RawCommand {
        let command = match self.from {
            Target::Id(id) => RawCommand::new("moveid").argument(id),
            Target::Range(range) => RawCommand::new("move").argument(range),
        };

        command.argument(self.to)
    }

    fn response(self, _: Frame) -> Result<Self::Response, TypedResponseError> {
        Ok(())
    }
}

/// `find` command.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Find {
    filter: Filter,
    sort: Option<Tag>,
    window: Option<SongRange>,
}

impl Find {
    /// Find all songs matching `filter`.
    pub fn new(filter: Filter) -> Self {
        Self {
            filter,
            sort: None,
            window: None,
        }
    }

    /// Sort the result by the given tag.
    ///
    /// This does some special-casing for certain tags, see the [MPD documentation][0] for details.
    ///
    /// # Panics
    ///
    /// This will panic when sending the command if you pass a malformed value using the
    /// [`Other`][error] variant.
    ///
    /// [0]: https://www.musicpd.org/doc/html/protocol.html#command-find
    /// [error]: crate::tag::Tag::Other
    pub fn sort(mut self, sort_by: Tag) -> Self {
        self.sort = Some(sort_by);
        self
    }

    /// Limit the result to the given window.
    pub fn window<R>(mut self, window: R) -> Self
    where
        R: RangeBounds<usize>,
    {
        self.window = Some(SongRange::new_usize(window));
        self
    }
}

impl Command for Find {
    type Response = Vec<res::Song>;

    fn command(&self) -> RawCommand {
        let mut command = RawCommand::new("find").argument(&self.filter);

        if let Some(sort) = &self.sort {
            command.add_argument("sort").unwrap();
            command
                .add_argument(sort.as_str())
                .expect("Invalid sort value");
        }

        if let Some(window) = self.window {
            command.add_argument("window").unwrap();
            command.add_argument(window).unwrap();
        }

        command
    }

    fn response(self, frame: Frame) -> Result<Self::Response, TypedResponseError> {
        res::Song::from_frame_multi(frame)
    }
}

/// `list` command.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct List<const N: usize = 0> {
    tag: Tag,
    filter: Option<Filter>,
    group_by: [Tag; N],
}

impl List<0> {
    /// List distinct values of `tag`.
    pub fn new(tag: Tag) -> List<0> {
        List {
            tag,
            filter: None,
            group_by: [],
        }
    }
}

impl<const N: usize> List<N> {
    /// Filter the songs being considered using the given `filter`.
    ///
    /// This will overwrite the filter if called multiple times.
    pub fn filter(mut self, filter: Filter) -> Self {
        self.filter = Some(filter);
        self
    }

    /// Group results by the given tag.
    ///
    /// This will overwrite the grouping if called multiple times.
    pub fn group_by<const M: usize>(self, group_by: [Tag; M]) -> List<M> {
        List {
            tag: self.tag,
            filter: self.filter,
            group_by,
        }
    }
}

impl<const N: usize> Command for List<N> {
    type Response = res::List<N>;

    fn command(&self) -> RawCommand {
        let mut command = RawCommand::new("list").argument(&self.tag);

        if let Some(filter) = self.filter.as_ref() {
            command.add_argument(filter).unwrap();
        }

        for group_by in &self.group_by {
            command.add_argument("group").unwrap();
            command.add_argument(group_by).unwrap();
        }

        command
    }

    fn response(self, frame: Frame) -> Result<Self::Response, TypedResponseError> {
        Ok(res::List::from_frame(self.tag, self.group_by, frame))
    }
}

/// `count` command without grouping.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Count {
    filter: Filter,
}

impl Count {
    /// Count the number and total playtime of all songs matching the given filter.
    pub fn new(filter: Filter) -> Count {
        Count { filter }
    }

    /// Group the results by the given tag.
    pub fn group_by(self, group_by: Tag) -> CountGrouped {
        CountGrouped {
            filter: Some(self.filter),
            group_by,
        }
    }
}

impl Command for Count {
    type Response = res::Count;

    fn command(&self) -> RawCommand {
        RawCommand::new("count").argument(&self.filter)
    }

    fn response(self, frame: Frame) -> Result<Self::Response, TypedResponseError> {
        res::Count::from_frame(frame)
    }
}

/// `count` command with grouping.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CountGrouped {
    group_by: Tag,
    filter: Option<Filter>,
}

impl CountGrouped {
    /// Count the number and total playtime of songs grouped by the given tag.
    pub fn new(group_by: Tag) -> CountGrouped {
        CountGrouped {
            group_by,
            filter: None,
        }
    }

    /// Only consider songs matching the given filter.
    ///
    /// If called multiple times, this will overwrite the filter.
    pub fn filter(mut self, filter: Filter) -> CountGrouped {
        self.filter = Some(filter);
        self
    }
}

impl Command for CountGrouped {
    type Response = Vec<(String, res::Count)>;

    fn command(&self) -> RawCommand {
        let mut cmd = RawCommand::new("count");

        if let Some(filter) = &self.filter {
            cmd.add_argument(filter).unwrap();
        }

        cmd.argument("group").argument(&self.group_by)
    }

    fn response(self, frame: Frame) -> Result<Self::Response, TypedResponseError> {
        res::Count::from_frame_grouped(frame, &self.group_by)
    }
}

/// `rename` command.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RenamePlaylist<'a> {
    from: &'a str,
    to: &'a str,
}

impl<'a> RenamePlaylist<'a> {
    /// Rename the playlist named `from` to `to`.
    pub fn new(from: &'a str, to: &'a str) -> Self {
        Self { from, to }
    }
}

impl<'a> Command for RenamePlaylist<'a> {
    type Response = ();

    fn command(&self) -> RawCommand {
        RawCommand::new("rename")
            .argument(self.from)
            .argument(self.to)
    }

    fn response(self, _: Frame) -> Result<Self::Response, TypedResponseError> {
        Ok(())
    }
}

/// `load` command.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LoadPlaylist<'a> {
    name: &'a str,
    range: Option<SongRange>,
}

impl<'a> LoadPlaylist<'a> {
    /// Load the playlist with the given name into the queue.
    pub fn name(name: &'a str) -> Self {
        Self { name, range: None }
    }

    /// Limit the loaded playlist to the given window.
    pub fn range<R>(mut self, range: R) -> Self
    where
        R: RangeBounds<usize>,
    {
        self.range = Some(SongRange::new_usize(range));
        self
    }
}

impl<'a> Command for LoadPlaylist<'a> {
    type Response = ();

    fn command(&self) -> RawCommand {
        let mut command = RawCommand::new("load").argument(self.name);

        if let Some(range) = self.range {
            command.add_argument(range).unwrap();
        }

        command
    }

    fn response(self, _: Frame) -> Result<Self::Response, TypedResponseError> {
        Ok(())
    }
}

/// `playlistadd` command.
///
/// If [`AddToPlaylist::at`] is not used, the song will be appended to the playlist.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AddToPlaylist<'a> {
    playlist: &'a str,
    song_url: &'a str,
    position: Option<SongPosition>,
}

impl<'a> AddToPlaylist<'a> {
    /// Add `song_url` to `playlist`.
    pub fn new(playlist: &'a str, song_url: &'a str) -> Self {
        Self {
            playlist,
            song_url,
            position: None,
        }
    }

    /// Add the URI at the given position in the queue.
    ///
    /// **NOTE**: Supported on protocol versions later than 0.23.3.
    pub fn at<P: Into<SongPosition>>(mut self, position: P) -> Self {
        self.position = Some(position.into());
        self
    }
}

impl<'a> Command for AddToPlaylist<'a> {
    type Response = ();

    fn command(&self) -> RawCommand {
        let mut command = RawCommand::new("playlistadd")
            .argument(self.playlist)
            .argument(self.song_url);

        if let Some(pos) = self.position {
            command.add_argument(pos).unwrap();
        }

        command
    }

    fn response(self, _: Frame) -> Result<Self::Response, TypedResponseError> {
        Ok(())
    }
}

/// `playlistdelete` command.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RemoveFromPlaylist<'a> {
    playlist: &'a str,
    target: PositionOrRange,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum PositionOrRange {
    Position(usize),
    Range(SongRange),
}

impl<'a> RemoveFromPlaylist<'a> {
    /// Delete the song at `position` from `playlist`.
    pub fn position(playlist: &'a str, position: usize) -> Self {
        RemoveFromPlaylist {
            playlist,
            target: PositionOrRange::Position(position),
        }
    }

    /// Delete the specified range of songs from `playlist`.
    pub fn range<R>(playlist: &'a str, range: R) -> Self
    where
        R: RangeBounds<SongPosition>,
    {
        RemoveFromPlaylist {
            playlist,
            target: PositionOrRange::Range(SongRange::new(range)),
        }
    }
}

impl<'a> Command for RemoveFromPlaylist<'a> {
    type Response = ();

    fn command(&self) -> RawCommand {
        let command = RawCommand::new("playlistdelete").argument(self.playlist);

        match self.target {
            PositionOrRange::Position(p) => command.argument(p),
            PositionOrRange::Range(r) => command.argument(r),
        }
    }

    fn response(self, _: Frame) -> Result<Self::Response, TypedResponseError> {
        Ok(())
    }
}

/// `playlistmove` command.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MoveInPlaylist<'a> {
    playlist: &'a str,
    from: usize,
    to: usize,
}

impl<'a> MoveInPlaylist<'a> {
    /// Move the song at `from` to `to` in the playlist named `playlist`.
    pub fn new(playlist: &'a str, from: usize, to: usize) -> Self {
        Self { playlist, from, to }
    }
}

impl<'a> Command for MoveInPlaylist<'a> {
    type Response = ();

    fn command(&self) -> RawCommand {
        RawCommand::new("playlistmove")
            .argument(self.playlist)
            .argument(self.from)
            .argument(self.to)
    }

    fn response(self, _: Frame) -> Result<Self::Response, TypedResponseError> {
        Ok(())
    }
}

/// `listallinfo` command.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ListAllIn<'a> {
    directory: &'a str,
}

impl<'a> ListAllIn<'a> {
    /// List all songs in the library.
    pub fn root() -> ListAllIn<'static> {
        ListAllIn { directory: "" }
    }

    /// List all songs beneath the given directory.
    pub fn directory(directory: &'a str) -> Self {
        Self { directory }
    }
}

impl<'a> Command for ListAllIn<'a> {
    type Response = Vec<res::Song>;

    fn command(&self) -> RawCommand {
        let mut command = RawCommand::new("listallinfo");

        if !self.directory.is_empty() {
            command.add_argument(self.directory).unwrap();
        }

        command
    }

    fn response(self, frame: Frame) -> Result<Self::Response, TypedResponseError> {
        res::Song::from_frame_multi(frame)
    }
}

/// Set the response binary length limit, in bytes.
///
/// This can dramatically speed up operations like [loading album art][crate::Client::album_art],
/// but may cause undesirable latency when using MPD over a slow connection.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SetBinaryLimit(pub usize);

impl Command for SetBinaryLimit {
    type Response = ();

    fn command(&self) -> RawCommand {
        RawCommand::new("binarylimit").argument(self.0)
    }

    fn response(self, _: Frame) -> Result<Self::Response, TypedResponseError> {
        Ok(())
    }
}

/// `albumart` command.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AlbumArt<'a> {
    uri: &'a str,
    offset: usize,
}

impl<'a> AlbumArt<'a> {
    /// Get the separate file album art for the given URI.
    pub fn new(uri: &'a str) -> Self {
        Self { uri, offset: 0 }
    }

    /// Load the resulting data starting from the given offset.
    pub fn offset(self, offset: usize) -> Self {
        Self { offset, ..self }
    }
}

impl<'a> Command for AlbumArt<'a> {
    type Response = Option<res::AlbumArt>;

    fn command(&self) -> RawCommand {
        RawCommand::new("albumart")
            .argument(self.uri)
            .argument(self.offset)
    }

    fn response(self, frame: Frame) -> Result<Self::Response, TypedResponseError> {
        res::AlbumArt::from_frame(frame)
    }
}

/// `readpicture` command.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AlbumArtEmbedded<'a> {
    uri: &'a str,
    offset: usize,
}

impl<'a> AlbumArtEmbedded<'a> {
    /// Get the separate file album art for the given URI.
    pub fn new(uri: &'a str) -> Self {
        Self { uri, offset: 0 }
    }

    /// Load the resulting data starting from the given offset.
    pub fn offset(self, offset: usize) -> Self {
        Self { offset, ..self }
    }
}

impl<'a> Command for AlbumArtEmbedded<'a> {
    type Response = Option<res::AlbumArt>;

    fn command(&self) -> RawCommand {
        RawCommand::new("readpicture")
            .argument(self.uri)
            .argument(self.offset)
    }

    fn response(self, frame: Frame) -> Result<Self::Response, TypedResponseError> {
        res::AlbumArt::from_frame(frame)
    }
}

/// Manage enabled tag types.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TagTypes<'a>(TagTypesAction<'a>);

impl<'a> TagTypes<'a> {
    /// Enable all tags.
    pub fn enable_all() -> TagTypes<'static> {
        TagTypes(TagTypesAction::EnableAll)
    }

    /// Disable all tags.
    pub fn disable_all() -> TagTypes<'static> {
        TagTypes(TagTypesAction::Clear)
    }

    /// Disable the given list of tags.
    ///
    /// # Panics
    ///
    /// Panics if called with an empty list of tags.
    pub fn disable(tags: &'a [Tag]) -> TagTypes<'a> {
        assert_ne!(tags.len(), 0, "The list of tags must not be empty");
        TagTypes(TagTypesAction::Disable(tags))
    }

    /// Enable the given list of tags.
    ///
    /// # Panics
    ///
    /// Panics if called with an empty list of tags.
    pub fn enable(tags: &'a [Tag]) -> TagTypes<'a> {
        assert_ne!(tags.len(), 0, "The list of tags must not be empty");
        TagTypes(TagTypesAction::Enable(tags))
    }
}

impl<'a> Command for TagTypes<'a> {
    type Response = ();

    fn command(&self) -> RawCommand {
        let mut cmd = RawCommand::new("tagtypes");

        match &self.0 {
            TagTypesAction::EnableAll => cmd.add_argument("all").unwrap(),
            TagTypesAction::Clear => cmd.add_argument("clear").unwrap(),
            TagTypesAction::Disable(tags) => {
                cmd.add_argument("disable").unwrap();

                for tag in tags.iter() {
                    cmd.add_argument(tag).unwrap();
                }
            }
            TagTypesAction::Enable(tags) => {
                cmd.add_argument("enable").unwrap();

                for tag in tags.iter() {
                    cmd.add_argument(tag).unwrap();
                }
            }
        }

        cmd
    }

    fn response(self, _: Frame) -> Result<Self::Response, TypedResponseError> {
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum TagTypesAction<'a> {
    EnableAll,
    Clear,
    Disable(&'a [Tag]),
    Enable(&'a [Tag]),
}

/// `sticker get` command
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StickerGet<'a> {
    uri: &'a str,
    name: &'a str,
}

impl<'a> StickerGet<'a> {
    /// Get the sticker `name` for the song at `uri`
    pub fn new(uri: &'a str, name: &'a str) -> Self {
        Self { uri, name }
    }
}

impl<'a> Command for StickerGet<'a> {
    type Response = res::StickerGet;

    fn command(&self) -> RawCommand {
        RawCommand::new("sticker")
            .argument("get")
            .argument("song")
            .argument(self.uri)
            .argument(self.name)
    }

    fn response(self, frame: Frame) -> Result<Self::Response, TypedResponseError> {
        res::StickerGet::from_frame(frame)
    }
}

/// `sticker set` command
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StickerSet<'a> {
    uri: &'a str,
    name: &'a str,
    value: &'a str,
}

impl<'a> StickerSet<'a> {
    /// Set the sticker `name` to `value` for the song at `uri`
    pub fn new(uri: &'a str, name: &'a str, value: &'a str) -> Self {
        Self { uri, name, value }
    }
}

impl<'a> Command for StickerSet<'a> {
    type Response = ();

    fn command(&self) -> RawCommand {
        RawCommand::new("sticker")
            .argument("set")
            .argument("song")
            .argument(self.uri)
            .argument(self.name)
            .argument(self.value)
    }

    fn response(self, _: Frame) -> Result<Self::Response, TypedResponseError> {
        Ok(())
    }
}

/// `sticker delete` command
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StickerDelete<'a> {
    uri: &'a str,
    name: &'a str,
}

impl<'a> StickerDelete<'a> {
    /// Delete the sticker `name` for the song at `uri`
    pub fn new(uri: &'a str, name: &'a str) -> Self {
        Self { uri, name }
    }
}

impl<'a> Command for StickerDelete<'a> {
    type Response = ();

    fn command(&self) -> RawCommand {
        RawCommand::new("sticker")
            .argument("delete")
            .argument("song")
            .argument(self.uri)
            .argument(self.name)
    }

    fn response(self, _: Frame) -> Result<Self::Response, TypedResponseError> {
        Ok(())
    }
}

/// `sticker list` command
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StickerList<'a> {
    uri: &'a str,
}

impl<'a> StickerList<'a> {
    /// Lists all stickers on the song at `uri`
    pub fn new(uri: &'a str) -> Self {
        Self { uri }
    }
}

impl<'a> Command for StickerList<'a> {
    type Response = res::StickerList;

    fn command(&self) -> RawCommand {
        RawCommand::new("sticker")
            .argument("list")
            .argument("song")
            .argument(self.uri)
    }

    fn response(self, frame: Frame) -> Result<Self::Response, TypedResponseError> {
        res::StickerList::from_frame(frame)
    }
}

/// Operator for full (filtered) version
/// of `sticker find` command
#[derive(Clone, Debug, PartialEq, Eq)]
enum StickerFindOperator {
    /// = operator
    Equals,
    /// < operator
    LessThan,
    /// > operator
    GreaterThan,
}

/// `sticker find` command
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StickerFind<'a> {
    uri: &'a str,
    name: &'a str,
    filter: Option<(StickerFindOperator, &'a str)>,
}

impl<'a> StickerFind<'a> {
    /// Lists all stickers on the song at `uri`
    pub fn new(uri: &'a str, name: &'a str) -> Self {
        Self {
            uri,
            name,
            filter: None,
        }
    }

    /// Find stickers where their value is equal to `value`
    pub fn where_eq(self, value: &'a str) -> Self {
        self.add_filter(StickerFindOperator::Equals, value)
    }

    /// Find stickers where their value is greater than `value`
    pub fn where_gt(self, value: &'a str) -> Self {
        self.add_filter(StickerFindOperator::GreaterThan, value)
    }

    /// Find stickers where their value is less than `value`
    pub fn where_lt(self, value: &'a str) -> Self {
        self.add_filter(StickerFindOperator::LessThan, value)
    }

    fn add_filter(self, operator: StickerFindOperator, value: &'a str) -> Self {
        Self {
            name: self.name,
            uri: self.uri,
            filter: Some((operator, value)),
        }
    }
}

impl<'a> Command for StickerFind<'a> {
    type Response = res::StickerFind;

    fn command(&self) -> RawCommand {
        let base = RawCommand::new("sticker")
            .argument("find")
            .argument("song")
            .argument(self.uri)
            .argument(self.name);

        if let Some((operator, value)) = self.filter.as_ref() {
            match operator {
                StickerFindOperator::Equals => base.argument("=").argument(value),
                StickerFindOperator::GreaterThan => base.argument(">").argument(value),
                StickerFindOperator::LessThan => base.argument("<").argument(value),
            }
        } else {
            base
        }
    }

    fn response(self, frame: Frame) -> Result<Self::Response, TypedResponseError> {
        res::StickerFind::from_frame(frame)
    }
}

/// `update` command.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Update<'a>(Option<&'a str>);

impl<'a> Update<'a> {
    /// Update the entire music database.
    pub fn new() -> Self {
        Update(None)
    }

    /// Restrict the update to the files below the given path.
    pub fn uri(self, uri: &'a str) -> Self {
        Self(Some(uri))
    }
}

impl<'a> Command for Update<'a> {
    type Response = u64;

    fn command(&self) -> RawCommand {
        let mut command = RawCommand::new("update");

        if let Some(uri) = self.0 {
            command.add_argument(uri).unwrap();
        }

        command
    }

    fn response(self, mut frame: Frame) -> Result<Self::Response, TypedResponseError> {
        value(&mut frame, "updating_db")
    }
}

impl<'a> Default for Update<'a> {
    fn default() -> Self {
        Update::new()
    }
}

/// `rescan` command.
///
/// Unlike the [`Update`] command, this will also scan files that don't appear to have changed
/// based on their modification time.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Rescan<'a>(Option<&'a str>);

impl<'a> Rescan<'a> {
    /// Rescan the entire music database.
    pub fn new() -> Self {
        Rescan(None)
    }

    /// Restrict the rescan to the files below the given path.
    pub fn uri(self, uri: &'a str) -> Self {
        Self(Some(uri))
    }
}

impl<'a> Command for Rescan<'a> {
    type Response = u64;

    fn command(&self) -> RawCommand {
        let mut command = RawCommand::new("rescan");

        if let Some(uri) = self.0 {
            command.add_argument(uri).unwrap();
        }

        command
    }

    fn response(self, mut frame: Frame) -> Result<Self::Response, TypedResponseError> {
        value(&mut frame, "updating_db")
    }
}

impl<'a> Default for Rescan<'a> {
    fn default() -> Self {
        Rescan::new()
    }
}

/// `readmessage` command.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ReadChannelMessages;

impl Command for ReadChannelMessages {
    type Response = Vec<(String, String)>;

    fn command(&self) -> RawCommand {
        RawCommand::new("readmessages")
    }

    fn response(self, frame: Frame) -> Result<Self::Response, TypedResponseError> {
        res::parse_channel_messages(frame)
    }
}

/// `channels` command.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ListChannels;

impl Command for ListChannels {
    type Response = Vec<String>;

    fn command(&self) -> RawCommand {
        RawCommand::new("channels")
    }

    fn response(self, frame: Frame) -> Result<Self::Response, TypedResponseError> {
        let mut response = Vec::with_capacity(frame.fields_len());
        for (key, value) in frame {
            if &*key != "channel" {
                return Err(TypedResponseError::unexpected_field("channel", &*key));
            }

            response.push(value);
        }

        Ok(response)
    }
}

/// `add` command.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AddToQueue<'a> {
    song_url: &'a str,
    position: Option<SongPosition>,
}

impl<'a>AddToQueue<'a> {
    /// Add `song_url` to `playlist`.
    pub fn new( song_url: &'a str) -> Self {
        Self {
            song_url,
            position: None,
        }
    }

    /// Add the URI at the given position in the queue.
    ///
    /// **NOTE**: Supported on protocol versions later than 0.23.3.
    pub fn at<P: Into<SongPosition>>(mut self, position: P) -> Self {
        self.position = Some(position.into());
        self
    }
}

impl<'a> Command for AddToQueue<'a> {
    type Response = ();

    fn command(&self) -> RawCommand {
        let mut command = RawCommand::new("add")
            .argument(self.song_url);

        if let Some(pos) = self.position {
            command.add_argument(pos).unwrap();
        }

        command
    }

    fn response(self, _: Frame) -> Result<Self::Response, TypedResponseError> {
        Ok(())
    }
}

/// `lsinfo` command.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GetLsInfo<'a> {
    pub directory_url: &'a str,
}

impl<'a> GetLsInfo<'a> {
    /// Add `song_url` to `playlist`.
    pub fn new(directory_url: &'a str) -> Self {
        Self {
            directory_url,
        }
    }

    /// List all storage items in the library.
    pub fn root() -> GetLsInfo<'static> {
        GetLsInfo { directory_url: "" }
    }

    /// List all storage items beneath the given directory.
    pub fn directory(directory_url: &'a str) -> Self {
        Self { directory_url }
    }
}

impl<'a> Command for GetLsInfo<'a> {
    type Response = Vec<res::StorageItem>;

    fn command(&self) -> RawCommand {
        RawCommand::new("lsinfo").argument(self.directory_url)
    }

    fn response(self, frame: Frame) -> Result<Self::Response, TypedResponseError> {
        res::StorageItem::from_frame_multi(frame)
    }
}

/// `sendmessage` command.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SendChannelMessage<'a> {
    channel: &'a str,
    message: &'a str,
}

impl<'a> SendChannelMessage<'a> {
    /// Send the given message to the given channel.
    pub fn new(channel: &'a str, message: &'a str) -> SendChannelMessage<'a> {
        SendChannelMessage { channel, message }
    }
}

impl<'a> Command for SendChannelMessage<'a> {
    type Response = ();

    fn command(&self) -> RawCommand {
        RawCommand::new("sendmessage")
            .argument(self.channel)
            .argument(self.message)
    }

    fn response(self, _frame: Frame) -> Result<Self::Response, TypedResponseError> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn range_arg() {
        let mut buf = BytesMut::new();

        SongRange::new_usize(2..4).render(&mut buf);
        assert_eq!(buf, "2:4");
        buf.clear();

        SongRange::new_usize(3..).render(&mut buf);
        assert_eq!(buf, "3:");
        buf.clear();

        SongRange::new_usize(2..=5).render(&mut buf);
        assert_eq!(buf, "2:6");
        buf.clear();

        SongRange::new_usize(..5).render(&mut buf);
        assert_eq!(buf, "0:5");
        buf.clear();

        SongRange::new_usize(..).render(&mut buf);
        assert_eq!(buf, "0:");
        buf.clear();

        SongRange::new_usize(1..=1).render(&mut buf);
        assert_eq!(buf, "1:2");
        buf.clear();

        SongRange::new_usize(..usize::MAX).render(&mut buf);
        assert_eq!(buf, format!("0:{}", usize::MAX));
        buf.clear();

        SongRange::new_usize(..=usize::MAX).render(&mut buf);
        assert_eq!(buf, format!("0:{}", usize::MAX));
        buf.clear();
    }

    #[test]
    fn command_queue() {
        assert_eq!(Queue.command(), RawCommand::new("playlistinfo"));
        assert_eq!(
            Queue::song(Song::Position(SongPosition(1))).command(),
            RawCommand::new("playlistinfo").argument("1")
        );
        assert_eq!(
            Queue::song(Song::Id(SongId(7))).command(),
            RawCommand::new("playlistid").argument("7")
        );
        assert_eq!(
            Queue::range(SongPosition(3)..SongPosition(18)).command(),
            RawCommand::new("playlistinfo").argument("3:18")
        );
    }

    #[test]
    fn command_crossfade() {
        assert_eq!(
            Crossfade(Duration::from_secs_f64(2.345)).command(),
            RawCommand::new("crossfade").argument("2")
        );
    }

    #[test]
    fn command_getplaylist() {
        assert_eq!(
            GetPlaylist("foo").command(),
            RawCommand::new("listplaylistinfo").argument("foo")
        );
    }

    #[test]
    fn command_volume() {
        assert_eq!(
            SetVolume(150).command(),
            RawCommand::new("setvol").argument("100")
        );
    }

    #[test]
    fn command_seek_to() {
        let duration = Duration::from_secs(2);

        assert_eq!(
            SeekTo(SongId(2).into(), duration).command(),
            RawCommand::new("seekid")
                .argument(SongId(2))
                .argument(duration)
        );

        assert_eq!(
            SeekTo(SongPosition(2).into(), duration).command(),
            RawCommand::new("seek")
                .argument(SongPosition(2))
                .argument(duration)
        );
    }

    #[test]
    fn command_seek() {
        let duration = Duration::from_secs(1);

        assert_eq!(
            Seek(SeekMode::Absolute(duration)).command(),
            RawCommand::new("seekcur").argument("1.000")
        );
        assert_eq!(
            Seek(SeekMode::Forward(duration)).command(),
            RawCommand::new("seekcur").argument("+1.000")
        );
        assert_eq!(
            Seek(SeekMode::Backward(duration)).command(),
            RawCommand::new("seekcur").argument("-1.000")
        );
    }

    #[test]
    fn command_shuffle() {
        assert_eq!(Shuffle::all().command(), RawCommand::new("shuffle"));
        assert_eq!(
            Shuffle::range(SongPosition(0)..SongPosition(2)).command(),
            RawCommand::new("shuffle").argument("0:2")
        );
    }

    #[test]
    fn command_play() {
        assert_eq!(Play::current().command(), RawCommand::new("play"));
        assert_eq!(
            Play::song(SongPosition(2)).command(),
            RawCommand::new("play").argument(SongPosition(2))
        );
        assert_eq!(
            Play::song(SongId(2)).command(),
            RawCommand::new("playid").argument(SongId(2))
        );
    }

    #[test]
    fn command_add() {
        let uri = "foo/bar.mp3";

        assert_eq!(
            Add::uri(uri).command(),
            RawCommand::new("addid").argument(uri)
        );
        assert_eq!(
            Add::uri(uri).at(5).command(),
            RawCommand::new("addid").argument(uri).argument("5")
        );
        assert_eq!(
            Add::uri(uri).before_current(5).command(),
            RawCommand::new("addid").argument(uri).argument("-5")
        );
        assert_eq!(
            Add::uri(uri).after_current(5).command(),
            RawCommand::new("addid").argument(uri).argument("+5")
        );
    }

    #[test]
    fn command_delete() {
        assert_eq!(
            Delete::id(SongId(2)).command(),
            RawCommand::new("deleteid").argument(SongId(2))
        );

        assert_eq!(
            Delete::position(SongPosition(2)).command(),
            RawCommand::new("delete").argument("2:3")
        );

        assert_eq!(
            Delete::range(SongPosition(2)..SongPosition(4)).command(),
            RawCommand::new("delete").argument("2:4")
        );
    }

    #[test]
    fn command_move() {
        assert_eq!(
            Move::position(SongPosition(2))
                .to_position(SongPosition(4))
                .command(),
            RawCommand::new("move").argument("2:3").argument("4")
        );

        assert_eq!(
            Move::id(SongId(2)).to_position(SongPosition(4)).command(),
            RawCommand::new("moveid")
                .argument(SongId(2))
                .argument(SongPosition(4))
        );

        assert_eq!(
            Move::range(SongPosition(3)..SongPosition(5))
                .to_position(SongPosition(4))
                .command(),
            RawCommand::new("move")
                .argument("3:5")
                .argument(SongPosition(4))
        );

        assert_eq!(
            Move::position(SongPosition(2)).after_current(3).command(),
            RawCommand::new("move").argument("2:3").argument("+3")
        );

        assert_eq!(
            Move::position(SongPosition(2)).before_current(3).command(),
            RawCommand::new("move").argument("2:3").argument("-3")
        );
    }

    #[test]
    fn command_find() {
        let filter = Filter::tag(Tag::Artist, "Foo");

        assert_eq!(
            Find::new(filter.clone()).command(),
            RawCommand::new("find").argument(filter.clone())
        );

        assert_eq!(
            Find::new(filter.clone()).window(..3).command(),
            RawCommand::new("find")
                .argument(filter.clone())
                .argument("window")
                .argument("0:3"),
        );

        assert_eq!(
            Find::new(filter.clone())
                .window(3..)
                .sort(Tag::Artist)
                .command(),
            RawCommand::new("find")
                .argument(filter)
                .argument("sort")
                .argument("Artist")
                .argument("window")
                .argument("3:")
        );
    }

    #[test]
    fn command_list() {
        assert_eq!(
            List::new(Tag::Album).command(),
            RawCommand::new("list").argument("Album")
        );

        let filter = Filter::tag(Tag::Artist, "Foo");
        assert_eq!(
            List::new(Tag::Album).filter(filter.clone()).command(),
            RawCommand::new("list").argument("Album").argument(filter)
        );

        let filter = Filter::tag(Tag::Artist, "Foo");
        assert_eq!(
            List::new(Tag::Title)
                .filter(filter.clone())
                .group_by([Tag::AlbumArtist, Tag::Album])
                .command(),
            RawCommand::new("list")
                .argument("Title")
                .argument(filter)
                .argument("group")
                .argument("AlbumArtist")
                .argument("group")
                .argument("Album")
        );
    }

    #[test]
    fn command_listallinfo() {
        assert_eq!(ListAllIn::root().command(), RawCommand::new("listallinfo"));

        assert_eq!(
            ListAllIn::directory("foo").command(),
            RawCommand::new("listallinfo").argument("foo")
        );
    }

    #[test]
    fn command_playlistdelete() {
        assert_eq!(
            RemoveFromPlaylist::position("foo", 5).command(),
            RawCommand::new("playlistdelete")
                .argument("foo")
                .argument("5"),
        );

        assert_eq!(
            RemoveFromPlaylist::range("foo", SongPosition(3)..SongPosition(6)).command(),
            RawCommand::new("playlistdelete")
                .argument("foo")
                .argument("3:6"),
        );
    }

    #[test]
    fn command_tagtypes() {
        assert_eq!(
            TagTypes::enable_all().command(),
            RawCommand::new("tagtypes").argument("all"),
        );

        assert_eq!(
            TagTypes::disable_all().command(),
            RawCommand::new("tagtypes").argument("clear"),
        );

        assert_eq!(
            TagTypes::disable(&[Tag::Album, Tag::Title]).command(),
            RawCommand::new("tagtypes")
                .argument("disable")
                .argument("Album")
                .argument("Title")
        );

        assert_eq!(
            TagTypes::enable(&[Tag::Album, Tag::Title]).command(),
            RawCommand::new("tagtypes")
                .argument("enable")
                .argument("Album")
                .argument("Title")
        );
    }

    #[test]
    fn command_get_enabled_tagtypes() {
        assert_eq!(GetEnabledTagTypes.command(), RawCommand::new("tagtypes"));
    }

    #[test]
    fn command_sticker_get() {
        assert_eq!(
            StickerGet::new("foo", "bar").command(),
            RawCommand::new("sticker")
                .argument("get")
                .argument("song")
                .argument("foo")
                .argument("bar")
        );
    }

    #[test]
    fn command_sticker_set() {
        assert_eq!(
            StickerSet::new("foo", "bar", "baz").command(),
            RawCommand::new("sticker")
                .argument("set")
                .argument("song")
                .argument("foo")
                .argument("bar")
                .argument("baz")
        );
    }

    #[test]
    fn command_sticker_delete() {
        assert_eq!(
            StickerDelete::new("foo", "bar").command(),
            RawCommand::new("sticker")
                .argument("delete")
                .argument("song")
                .argument("foo")
                .argument("bar")
        );
    }

    #[test]
    fn command_sticker_list() {
        assert_eq!(
            StickerList::new("foo").command(),
            RawCommand::new("sticker")
                .argument("list")
                .argument("song")
                .argument("foo")
        );
    }

    #[test]
    fn command_sticker_find() {
        assert_eq!(
            StickerFind::new("foo", "bar").command(),
            RawCommand::new("sticker")
                .argument("find")
                .argument("song")
                .argument("foo")
                .argument("bar")
        );

        assert_eq!(
            StickerFind::new("foo", "bar").where_eq("baz").command(),
            RawCommand::new("sticker")
                .argument("find")
                .argument("song")
                .argument("foo")
                .argument("bar")
                .argument("=")
                .argument("baz")
        );
    }

    #[test]
    fn command_update() {
        assert_eq!(Update::new().command(), RawCommand::new("update"));

        assert_eq!(
            Update::new().uri("folder").command(),
            RawCommand::new("update").argument("folder")
        )
    }

    #[test]
    fn command_rescan() {
        assert_eq!(Rescan::new().command(), RawCommand::new("rescan"));

        assert_eq!(
            Rescan::new().uri("folder").command(),
            RawCommand::new("rescan").argument("folder")
        )
    }

    #[test]
    fn command_subscribe() {
        assert_eq!(
            SubscribeToChannel("hello").command(),
            RawCommand::new("subscribe").argument("hello")
        );
    }

    #[test]
    fn command_unsubscribe() {
        assert_eq!(
            UnsubscribeFromChannel("hello").command(),
            RawCommand::new("unsubscribe").argument("hello")
        );
    }

    #[test]
    fn command_read_messages() {
        assert_eq!(
            ReadChannelMessages.command(),
            RawCommand::new("readmessages")
        );
    }

    #[test]
    fn command_list_channels() {
        assert_eq!(ListChannels.command(), RawCommand::new("channels"));
    }

    #[test]
    fn command_send_message() {
        assert_eq!(
            SendChannelMessage::new("foo", "bar").command(),
            RawCommand::new("sendmessage")
                .argument("foo")
                .argument("bar")
        );
    }

    #[test]
    fn command_add_to_queue() {
        assert_eq!(
            AddToQueue::new("foo").command(),
            RawCommand::new("add")
                .argument("foo")
        );

        assert_eq!(
            AddToQueue::new("foo").at(SongPosition(0)).command(),
            RawCommand::new("add")
                .argument("foo")
                .argument("0")
        );
    }

    #[test]
    fn command_list_partitions() {
        assert_eq!(
            ListPartitions.command(),
            RawCommand::new("listpartitions")
        );
    }
}
