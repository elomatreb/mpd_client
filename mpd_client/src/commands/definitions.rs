//! Definitions of commands.

use mpd_protocol::command::Argument;

use std::borrow::Cow;
use std::cmp::min;
use std::ops::{Bound, RangeBounds};
use std::time::Duration;

use crate::commands::{
    responses as res, Command, SeekMode, SingleMode, Song, SongId, SongPosition,
};
use crate::raw::RawCommand;
use crate::tag::Tag;
use crate::Filter;

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
            #[doc = concat!("`", $command, "` command.")],
            pub struct $name;
        );

        impl Command for $name {
            type Response = $response;

            fn into_command(self) -> RawCommand {
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
            #[doc = concat!("`", $command, "` command.")],
            pub struct $name(pub $argtype);
        );

        impl Command for $name {
            type Response = $response;

            fn into_command(self) -> RawCommand {
                RawCommand::new($command)
                    .argument(self.0)
            }
        }
    };
}

argless_command!(Ping, "ping", res::Empty);

argless_command!(Next, "next", res::Empty);
argless_command!(Previous, "previous", res::Empty);
argless_command!(Stop, "stop", res::Empty);
argless_command!(ClearQueue, "clear", res::Empty);

argless_command!(Status, "status", res::Status);
argless_command!(Stats, "stats", res::Stats);

argless_command!(Queue, "playlistinfo", Vec<res::SongInQueue>);
argless_command!(CurrentSong, "currentsong", Option<res::SongInQueue>);

argless_command!(GetPlaylists, "listplaylists", Vec<res::Playlist>);

argless_command!(EnabledTagTypes, "tagtypes", Vec<Tag>);

single_arg_command!(SetRandom, bool, "random", res::Empty);
single_arg_command!(SetConsume, bool, "consume", res::Empty);
single_arg_command!(SetRepeat, bool, "repeat", res::Empty);
single_arg_command!(SetPause, bool, "pause", res::Empty);

single_arg_command!(SaveQueueAsPlaylist, String, "save", res::Empty);
single_arg_command!(DeletePlaylist, String, "rm", res::Empty);
single_arg_command!(GetPlaylist, String, "listplaylistinfo", Vec<res::Song>);
single_arg_command!(ClearPlaylist, String, "playlistclear", res::Empty);

/// `crossfade` command.
///
/// The given duration is truncated to the seconds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Crossfade(pub Duration);

impl Command for Crossfade {
    type Response = res::Empty;

    fn into_command(self) -> RawCommand {
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

    fn into_command(self) -> RawCommand {
        let volume = min(self.0, 100);
        RawCommand::new("setvol").argument(volume.to_string())
    }
}

/// `single` command.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SetSingle(pub SingleMode);

impl Command for SetSingle {
    type Response = res::Empty;

    fn into_command(self) -> RawCommand {
        let single = match self.0 {
            SingleMode::Disabled => "0",
            SingleMode::Enabled => "1",
            SingleMode::Oneshot => "oneshot",
        };

        RawCommand::new("single").argument(single)
    }
}

/// `seek` and `seekid` commands.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SeekTo(pub Song, pub Duration);

impl Command for SeekTo {
    type Response = res::Empty;

    fn into_command(self) -> RawCommand {
        let command = match self.0 {
            Song::Position(pos) => RawCommand::new("seek").argument(pos),
            Song::Id(id) => RawCommand::new("seekid").argument(id),
        };

        command.argument(self.1)
    }
}

/// `seekcur` command.
///
/// Seek in the current song.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Seek(pub SeekMode);

impl Command for Seek {
    type Response = res::Empty;

    fn into_command(self) -> RawCommand {
        let time = match self.0 {
            SeekMode::Absolute(pos) => format!("{:.3}", pos.as_secs_f64()),
            SeekMode::Forward(time) => format!("+{:.3}", time.as_secs_f64()),
            SeekMode::Backward(time) => format!("-{:.3}", time.as_secs_f64()),
        };

        RawCommand::new("seekcur").argument(time)
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
    type Response = res::Empty;

    fn into_command(self) -> RawCommand {
        match self.0 {
            None => RawCommand::new("play"),
            Some(Song::Position(pos)) => RawCommand::new("play").argument(pos),
            Some(Song::Id(id)) => RawCommand::new("playid").argument(id),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PositionOrRelative {
    Absolute(SongPosition),
    BeforeCurrent(usize),
    AfterCurrent(usize),
}

impl Argument for PositionOrRelative {
    fn render(self) -> Cow<'static, str> {
        match self {
            PositionOrRelative::Absolute(pos) => pos.render(),
            PositionOrRelative::AfterCurrent(x) => Cow::Owned(format!("+{}", x)),
            PositionOrRelative::BeforeCurrent(x) => Cow::Owned(format!("-{}", x)),
        }
    }
}

/// `addid` command.
///
/// Add a song to the queue, returning its ID. If neither of [`Add::at`], [`Add::before_current`],
/// or [`Add::after_current`] is used, the song will be appended to the queue.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Add {
    uri: String,
    position: Option<PositionOrRelative>,
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

impl Command for Add {
    type Response = SongId;

    fn into_command(self) -> RawCommand {
        let mut command = RawCommand::new("addid").argument(self.uri);

        if let Some(pos) = self.position {
            command.add_argument(pos).unwrap();
        }

        command
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct SongRange {
    from: usize,
    to: Option<usize>,
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

impl SongRange {
    fn new_usize<R: RangeBounds<usize>>(range: R) -> Self {
        let from = match range.start_bound() {
            Bound::Excluded(pos) => pos + 1,
            Bound::Included(pos) => *pos,
            Bound::Unbounded => 0,
        };

        let to = match range.end_bound() {
            Bound::Excluded(pos) => Some(*pos),
            Bound::Included(pos) => Some(pos + 1),
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
    fn render(self) -> Cow<'static, str> {
        Cow::Owned(match self.to {
            Some(to) => format!("{}:{}", self.from, to),
            None => format!("{}:", self.from),
        })
    }
}

impl Command for Delete {
    type Response = res::Empty;

    fn into_command(self) -> RawCommand {
        match self.0 {
            Target::Id(id) => RawCommand::new("deleteid").argument(id),
            Target::Range(range) => RawCommand::new("delete").argument(range),
        }
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
    /// **NOTE**: The given range must have an end. If a range with an open end is passed, this
    /// function will panic.
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
    type Response = res::Empty;

    fn into_command(self) -> RawCommand {
        let command = match self.from {
            Target::Id(id) => RawCommand::new("moveid").argument(id),
            Target::Range(range) => RawCommand::new("move").argument(range),
        };

        command.argument(self.to)
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

    fn into_command(self) -> RawCommand {
        let mut command = RawCommand::new("find").argument(self.filter);

        if let Some(sort) = self.sort {
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
}

/// `list` command.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct List {
    tag: Tag,
    filter: Option<Filter>,
    group_by: Option<Tag>,
}

impl List {
    /// List distinct values of `tag`.
    pub fn new(tag: Tag) -> Self {
        List {
            tag,
            filter: None,
            group_by: None,
        }
    }

    /// Filter the songs being considered using the given `filter`.
    pub fn filter(mut self, filter: Filter) -> Self {
        self.filter = Some(filter);
        self
    }

    /// Group results by the given tag.
    pub fn group_by(mut self, group_by: Tag) -> Self {
        self.group_by = Some(group_by);
        self
    }
}

impl Command for List {
    type Response = res::List;

    fn into_command(self) -> RawCommand {
        let mut command = RawCommand::new("list").argument(self.tag);

        if let Some(filter) = self.filter {
            command.add_argument(filter).unwrap();
        }

        if let Some(group_by) = self.group_by {
            command.add_argument("group").unwrap();
            command.add_argument(group_by).unwrap();
        }

        command
    }
}

/// `rename` command.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RenamePlaylist {
    from: String,
    to: String,
}

impl RenamePlaylist {
    /// Rename the playlist named `from` to `to`.
    pub fn new(from: String, to: String) -> Self {
        Self { from, to }
    }
}

impl Command for RenamePlaylist {
    type Response = res::Empty;

    fn into_command(self) -> RawCommand {
        RawCommand::new("rename")
            .argument(self.from)
            .argument(self.to)
    }
}

/// `load` command.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LoadPlaylist {
    name: String,
    range: Option<SongRange>,
}

impl LoadPlaylist {
    /// Load the playlist with the given name into the queue.
    pub fn name(name: String) -> Self {
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

impl Command for LoadPlaylist {
    type Response = res::Empty;

    fn into_command(self) -> RawCommand {
        let mut command = RawCommand::new("load").argument(self.name);

        if let Some(range) = self.range {
            command.add_argument(range).unwrap();
        }

        command
    }
}

/// `playlistadd` command.
///
/// If [`AddToPlaylist::at`] is not used, the song will be appended to the playlist.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AddToPlaylist {
    playlist: String,
    song_url: String,
    position: Option<SongPosition>,
}

impl AddToPlaylist {
    /// Add `song_url` to `playlist`.
    pub fn new(playlist: String, song_url: String) -> Self {
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

impl Command for AddToPlaylist {
    type Response = res::Empty;

    fn into_command(self) -> RawCommand {
        let mut command = RawCommand::new("playlistadd")
            .argument(self.playlist)
            .argument(self.song_url);

        if let Some(pos) = self.position {
            command.add_argument(pos).unwrap();
        }

        command
    }
}

/// `playlistdelete` command.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RemoveFromPlaylist {
    playlist: String,
    target: PositionOrRange,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum PositionOrRange {
    Position(usize),
    Range(SongRange),
}

impl RemoveFromPlaylist {
    /// Delete the song at `position` from `playlist`.
    pub fn position(playlist: String, position: usize) -> Self {
        RemoveFromPlaylist {
            playlist,
            target: PositionOrRange::Position(position),
        }
    }

    /// Delete the specified range of songs from `playlist`.
    pub fn range<R>(playlist: String, range: R) -> Self
    where
        R: RangeBounds<SongPosition>,
    {
        RemoveFromPlaylist {
            playlist,
            target: PositionOrRange::Range(SongRange::new(range)),
        }
    }
}

impl Command for RemoveFromPlaylist {
    type Response = res::Empty;

    fn into_command(self) -> RawCommand {
        let command = RawCommand::new("playlistdelete").argument(self.playlist);

        match self.target {
            PositionOrRange::Position(p) => command.argument(p.to_string()),
            PositionOrRange::Range(r) => command.argument(r),
        }
    }
}

/// `playlistmove` command.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MoveInPlaylist {
    playlist: String,
    from: usize,
    to: usize,
}

impl MoveInPlaylist {
    /// Move the song at `from` to `to` in the playlist named `playlist`.
    pub fn new(playlist: String, from: usize, to: usize) -> Self {
        Self { playlist, from, to }
    }
}

impl Command for MoveInPlaylist {
    type Response = res::Empty;

    fn into_command(self) -> RawCommand {
        RawCommand::new("playlistmove")
            .argument(self.playlist)
            .argument(self.from.to_string())
            .argument(self.to.to_string())
    }
}

/// `listallinfo` command.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ListAllIn {
    directory: String,
}

impl ListAllIn {
    /// List all songs in the library.
    pub fn root() -> Self {
        Self {
            directory: String::new(),
        }
    }

    /// List all songs beneath the given directory.
    pub fn directory(directory: String) -> Self {
        Self { directory }
    }
}

impl Command for ListAllIn {
    type Response = Vec<res::Song>;

    fn into_command(self) -> RawCommand {
        let mut command = RawCommand::new("listallinfo");

        if !self.directory.is_empty() {
            command.add_argument(self.directory).unwrap();
        }

        command
    }
}

/// Set the response binary length limit, in bytes.
///
/// This can dramatically speed up operations like [loading album art][crate::Client::album_art],
/// but may cause undesirable latency when using MPD over a slow connection.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SetBinaryLimit(pub usize);

impl Command for SetBinaryLimit {
    type Response = res::Empty;

    fn into_command(self) -> RawCommand {
        RawCommand::new("binarylimit").argument(self.0.to_string())
    }
}

/// `albumart` command.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AlbumArt {
    uri: String,
    offset: usize,
}

impl AlbumArt {
    /// Get the separate file album art for the given URI.
    pub fn new(uri: String) -> Self {
        Self { uri, offset: 0 }
    }

    /// Load the resulting data starting from the given offset.
    pub fn offset(self, offset: usize) -> Self {
        Self { offset, ..self }
    }
}

impl Command for AlbumArt {
    type Response = Option<res::AlbumArt>;

    fn into_command(self) -> RawCommand {
        RawCommand::new("albumart")
            .argument(self.uri)
            .argument(self.offset.to_string())
    }
}

/// `readpicture` command.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AlbumArtEmbedded {
    uri: String,
    offset: usize,
}

impl AlbumArtEmbedded {
    /// Get the separate file album art for the given URI.
    pub fn new(uri: String) -> Self {
        Self { uri, offset: 0 }
    }

    /// Load the resulting data starting from the given offset.
    pub fn offset(self, offset: usize) -> Self {
        Self { offset, ..self }
    }
}

impl Command for AlbumArtEmbedded {
    type Response = Option<res::AlbumArt>;

    fn into_command(self) -> RawCommand {
        RawCommand::new("readpicture")
            .argument(self.uri)
            .argument(self.offset.to_string())
    }
}

/// Manage enabled tag types.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TagTypes(TagTypesAction);

impl TagTypes {
    /// Enable all tags.
    pub fn enable_all() -> TagTypes {
        TagTypes(TagTypesAction::EnableAll)
    }

    /// Disable all tags.
    pub fn disable_all() -> TagTypes {
        TagTypes(TagTypesAction::Clear)
    }

    /// Disable the given list of tags.
    ///
    /// # Panics
    ///
    /// Panics if called with an empty list of tags.
    pub fn disable(tags: Vec<Tag>) -> TagTypes {
        assert_ne!(tags.len(), 0, "The list of tags must not be empty");
        TagTypes(TagTypesAction::Disable(tags))
    }

    /// Enable the given list of tags.
    ///
    /// # Panics
    ///
    /// Panics if called with an empty list of tags.
    pub fn enable(tags: Vec<Tag>) -> TagTypes {
        assert_ne!(tags.len(), 0, "The list of tags must not be empty");
        TagTypes(TagTypesAction::Enable(tags))
    }
}

impl Command for TagTypes {
    type Response = res::Empty;

    fn into_command(self) -> RawCommand {
        let mut cmd = RawCommand::new("tagtypes");

        match self.0 {
            TagTypesAction::EnableAll => cmd.add_argument("all").unwrap(),
            TagTypesAction::Clear => cmd.add_argument("clear").unwrap(),
            TagTypesAction::Disable(tags) => {
                cmd.add_argument("disable").unwrap();

                for tag in tags {
                    cmd.add_argument(tag).unwrap();
                }
            }
            TagTypesAction::Enable(tags) => {
                cmd.add_argument("enable").unwrap();

                for tag in tags {
                    cmd.add_argument(tag).unwrap();
                }
            }
        }

        cmd
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum TagTypesAction {
    EnableAll,
    Clear,
    Disable(Vec<Tag>),
    Enable(Vec<Tag>),
}

/// `sticker get` command
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StickerGet {
    uri: String,
    name: String,
}

impl StickerGet {
    /// Get the sticker `name` for the song at `uri`
    pub fn new(uri: String, name: String) -> Self {
        Self { uri, name }
    }
}

impl Command for StickerGet {
    type Response = res::StickerGet;

    fn into_command(self) -> RawCommand {
        RawCommand::new("sticker")
            .argument("get")
            .argument("song")
            .argument(self.uri)
            .argument(self.name)
    }
}

/// `sticker set` command
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StickerSet {
    uri: String,
    name: String,
    value: String,
}

impl StickerSet {
    /// Set the sticker `name` to `value` for the song at `uri`
    pub fn new(uri: String, name: String, value: String) -> Self {
        Self { uri, name, value }
    }
}

impl Command for StickerSet {
    type Response = res::Empty;

    fn into_command(self) -> RawCommand {
        RawCommand::new("sticker")
            .argument("set")
            .argument("song")
            .argument(self.uri)
            .argument(self.name)
            .argument(self.value)
    }
}

/// `sticker delete` command
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StickerDelete {
    uri: String,
    name: String,
}

impl StickerDelete {
    /// Delete the sticker `name` for the song at `uri`
    pub fn new(uri: String, name: String) -> Self {
        Self { uri, name }
    }
}

impl Command for StickerDelete {
    type Response = res::Empty;

    fn into_command(self) -> RawCommand {
        RawCommand::new("sticker")
            .argument("delete")
            .argument("song")
            .argument(self.uri)
            .argument(self.name)
    }
}

/// `sticker list` command
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StickerList {
    uri: String,
}

impl StickerList {
    /// Lists all stickers on the song at `uri`
    pub fn new(uri: String) -> Self {
        Self { uri }
    }
}

impl Command for StickerList {
    type Response = res::StickerList;

    fn into_command(self) -> RawCommand {
        RawCommand::new("sticker")
            .argument("list")
            .argument("song")
            .argument(self.uri)
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
pub struct StickerFind {
    uri: String,
    name: String,
    filter: Option<(StickerFindOperator, String)>
}

impl StickerFind {
    /// Lists all stickers on the song at `uri`
    pub fn new(uri: String, name: String) -> Self {
        Self {
            uri,
            name,
            filter: None
        }
    }

    /// Find stickers where their value is equal to `value`
    pub fn where_eq(self, value: String) -> Self {
        self.add_filter(StickerFindOperator::Equals, value)
    }

    /// Find stickers where their value is greater than `value`
    pub fn where_gt(self, value: String) -> Self {
        self.add_filter(StickerFindOperator::GreaterThan, value)
    }

    /// Find stickers where their value is less than `value`
    pub fn where_lt(self, value: String) -> Self {
        self.add_filter(StickerFindOperator::LessThan, value)
    }

    fn add_filter(self, operator: StickerFindOperator, value: String) -> Self {
        Self {
            name: self.name,
            uri: self.uri,
            filter: Some((operator, value))
        }
    }
}

impl Command for StickerFind {
    type Response = res::StickerFind;

    fn into_command(self) -> RawCommand {
        let base = RawCommand::new("sticker")
            .argument("find")
            .argument("song")
            .argument(self.uri)
            .argument(self.name);

        if let Some((operator, value)) = self.filter {
            match operator {
                StickerFindOperator::Equals => base.argument("=").argument(value),
                StickerFindOperator::GreaterThan => base.argument(">").argument(value),
                StickerFindOperator::LessThan => base.argument("<").argument(value),
            }
        } else {
            base
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn range_arg() {
        assert_eq!(SongRange::new_usize(2..4).render(), "2:4");
        assert_eq!(SongRange::new_usize(3..).render(), "3:");
        assert_eq!(SongRange::new_usize(2..=5).render(), "2:6");
        assert_eq!(SongRange::new_usize(..5).render(), "0:5");
        assert_eq!(SongRange::new_usize(..).render(), "0:");
        assert_eq!(SongRange::new_usize(1..=1).render(), "1:2");
    }

    #[test]
    fn command_crossfade() {
        assert_eq!(
            Crossfade(Duration::from_secs_f64(2.345)).into_command(),
            RawCommand::new("crossfade").argument("2")
        );
    }

    #[test]
    fn command_volume() {
        assert_eq!(
            SetVolume(150).into_command(),
            RawCommand::new("setvol").argument("100")
        );
    }

    #[test]
    fn command_seek_to() {
        let duration = Duration::from_secs(2);

        assert_eq!(
            SeekTo(SongId(2).into(), duration).into_command(),
            RawCommand::new("seekid")
                .argument(SongId(2))
                .argument(duration)
        );

        assert_eq!(
            SeekTo(SongPosition(2).into(), duration).into_command(),
            RawCommand::new("seek")
                .argument(SongPosition(2))
                .argument(duration)
        );
    }

    #[test]
    fn command_seek() {
        let duration = Duration::from_secs(1);

        assert_eq!(
            Seek(SeekMode::Absolute(duration)).into_command(),
            RawCommand::new("seekcur").argument("1.000")
        );
        assert_eq!(
            Seek(SeekMode::Forward(duration)).into_command(),
            RawCommand::new("seekcur").argument("+1.000")
        );
        assert_eq!(
            Seek(SeekMode::Backward(duration)).into_command(),
            RawCommand::new("seekcur").argument("-1.000")
        );
    }

    #[test]
    fn command_play() {
        assert_eq!(Play::current().into_command(), RawCommand::new("play"));
        assert_eq!(
            Play::song(SongPosition(2)).into_command(),
            RawCommand::new("play").argument(SongPosition(2))
        );
        assert_eq!(
            Play::song(SongId(2)).into_command(),
            RawCommand::new("playid").argument(SongId(2))
        );
    }

    #[test]
    fn command_add() {
        let uri = String::from("foo/bar.mp3");

        assert_eq!(
            Add::uri(uri.clone()).into_command(),
            RawCommand::new("addid").argument(uri.clone())
        );
        assert_eq!(
            Add::uri(uri.clone()).at(5).into_command(),
            RawCommand::new("addid").argument(uri.clone()).argument("5")
        );
        assert_eq!(
            Add::uri(uri.clone()).before_current(5).into_command(),
            RawCommand::new("addid")
                .argument(uri.clone())
                .argument("-5")
        );
        assert_eq!(
            Add::uri(uri.clone()).after_current(5).into_command(),
            RawCommand::new("addid").argument(uri).argument("+5")
        );
    }

    #[test]
    fn command_delete() {
        assert_eq!(
            Delete::id(SongId(2)).into_command(),
            RawCommand::new("deleteid").argument(SongId(2))
        );

        assert_eq!(
            Delete::position(SongPosition(2)).into_command(),
            RawCommand::new("delete").argument("2:3")
        );

        assert_eq!(
            Delete::range(SongPosition(2)..SongPosition(4)).into_command(),
            RawCommand::new("delete").argument("2:4")
        );
    }

    #[test]
    fn command_move() {
        assert_eq!(
            Move::position(SongPosition(2))
                .to_position(SongPosition(4))
                .into_command(),
            RawCommand::new("move").argument("2:3").argument("4")
        );

        assert_eq!(
            Move::id(SongId(2))
                .to_position(SongPosition(4))
                .into_command(),
            RawCommand::new("moveid")
                .argument(SongId(2))
                .argument(SongPosition(4))
        );

        assert_eq!(
            Move::range(SongPosition(3)..SongPosition(5))
                .to_position(SongPosition(4))
                .into_command(),
            RawCommand::new("move")
                .argument("3:5")
                .argument(SongPosition(4))
        );

        assert_eq!(
            Move::position(SongPosition(2))
                .after_current(3)
                .into_command(),
            RawCommand::new("move").argument("2:3").argument("+3")
        );

        assert_eq!(
            Move::position(SongPosition(2))
                .before_current(3)
                .into_command(),
            RawCommand::new("move").argument("2:3").argument("-3")
        );
    }

    #[test]
    fn command_find() {
        let filter = Filter::tag(Tag::Artist, "Foo");

        assert_eq!(
            Find::new(filter.clone()).into_command(),
            RawCommand::new("find").argument(filter.clone())
        );

        assert_eq!(
            Find::new(filter.clone()).window(..3).into_command(),
            RawCommand::new("find")
                .argument(filter.clone())
                .argument("window")
                .argument("0:3"),
        );

        assert_eq!(
            Find::new(filter.clone())
                .window(3..)
                .sort(Tag::Artist)
                .into_command(),
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
            List::new(Tag::Album).into_command(),
            RawCommand::new("list").argument("Album")
        );

        let filter = Filter::tag(Tag::Artist, "Foo");
        assert_eq!(
            List::new(Tag::Album).filter(filter.clone()).into_command(),
            RawCommand::new("list").argument("Album").argument(filter)
        );

        let filter = Filter::tag(Tag::Artist, "Foo");
        assert_eq!(
            List::new(Tag::Album)
                .filter(filter.clone())
                .group_by(Tag::AlbumArtist)
                .into_command(),
            RawCommand::new("list")
                .argument("Album")
                .argument(filter)
                .argument("group")
                .argument("AlbumArtist")
        );
    }

    #[test]
    fn command_listallinfo() {
        assert_eq!(
            ListAllIn::root().into_command(),
            RawCommand::new("listallinfo")
        );

        assert_eq!(
            ListAllIn::directory(String::from("foo")).into_command(),
            RawCommand::new("listallinfo").argument("foo")
        );
    }

    #[test]
    fn command_playlistdelete() {
        assert_eq!(
            RemoveFromPlaylist::position(String::from("foo"), 5).into_command(),
            RawCommand::new("playlistdelete")
                .argument("foo")
                .argument("5"),
        );

        assert_eq!(
            RemoveFromPlaylist::range(String::from("foo"), SongPosition(3)..SongPosition(6))
                .into_command(),
            RawCommand::new("playlistdelete")
                .argument("foo")
                .argument("3:6"),
        );
    }

    #[test]
    fn command_tagtypes() {
        assert_eq!(
            TagTypes::enable_all().into_command(),
            RawCommand::new("tagtypes").argument("all"),
        );

        assert_eq!(
            TagTypes::disable_all().into_command(),
            RawCommand::new("tagtypes").argument("clear"),
        );

        assert_eq!(
            TagTypes::disable(vec![Tag::Album, Tag::Title]).into_command(),
            RawCommand::new("tagtypes")
                .argument("disable")
                .argument("Album")
                .argument("Title")
        );

        assert_eq!(
            TagTypes::enable(vec![Tag::Album, Tag::Title]).into_command(),
            RawCommand::new("tagtypes")
                .argument("enable")
                .argument("Album")
                .argument("Title")
        );
    }

    #[test]
    fn command_enabled_tagtypes() {
        assert_eq!(EnabledTagTypes.into_command(), RawCommand::new("tagtypes"));
    }

    #[test]
    fn command_sticker_get() {
        assert_eq!(
            StickerGet::new("foo".to_string(), "bar".to_string()).into_command(),
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
            StickerSet::new("foo".to_string(), "bar".to_string(), "baz".to_string()).into_command(),
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
            StickerDelete::new("foo".to_string(), "bar".to_string()).into_command(),
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
            StickerList::new("foo".to_string()).into_command(),
            RawCommand::new("sticker")
                .argument("list")
                .argument("song")
                .argument("foo")
        );
    }

    #[test]
    fn command_sticker_find() {
        assert_eq!(
            StickerFind::new("foo".to_string(), "bar".to_string()).into_command(),
            RawCommand::new("sticker")
                .argument("find")
                .argument("song")
                .argument("foo")
                .argument("bar")
        );

        assert_eq!(
            StickerFind::new("foo".to_string(), "bar".to_string())
                .where_eq("baz".to_string())
                .into_command(),
            RawCommand::new("sticker")
                .argument("find")
                .argument("song")
                .argument("foo")
                .argument("bar")
                .argument("=")
                .argument("baz")
        );
    }
}
