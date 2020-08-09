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
            #[doc = concat!("`", $command, "` command.")],
            pub struct $name(pub $argtype);
        );

        impl Command for $name {
            type Response = $response;

            fn to_command(self) -> RawCommand {
                RawCommand::new($command)
                    .argument(self.0)
            }
        }
    };
}

argless_command!(Next, "next", res::Empty);
argless_command!(Previous, "previous", res::Empty);
argless_command!(Stop, "stop", res::Empty);
argless_command!(ClearQueue, "clear", res::Empty);

argless_command!(Status, "status", res::Status);
argless_command!(Stats, "stats", res::Stats);

argless_command!(Queue, "playlistinfo", Vec<res::SongInQueue>);
argless_command!(CurrentSong, "currentsong", Option<res::SongInQueue>);

argless_command!(GetPlaylists, "listplaylists", Vec<res::Playlist>);

single_arg_command!(SetRandom, bool, "random", res::Empty);
single_arg_command!(SetConsume, bool, "consume", res::Empty);
single_arg_command!(SetRepeat, bool, "repeat", res::Empty);
single_arg_command!(SetPause, bool, "pause", res::Empty);

single_arg_command!(Password, String, "password", res::Empty);

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

/// `seek` and `seekid` commands.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SeekTo(pub Song, pub Duration);

impl Command for SeekTo {
    type Response = res::Empty;

    fn to_command(self) -> RawCommand {
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
            Some(Song::Position(pos)) => RawCommand::new("play").argument(pos),
            Some(Song::Id(id)) => RawCommand::new("playid").argument(id),
        }
    }
}

/// `addid` command.
///
/// Add a song to the queue, returning its ID. If [`Add::at`] is not used, the song will be appended to
/// the queue.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Add {
    uri: String,
    position: Option<SongPosition>,
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
        self.position = Some(position.into());
        self
    }
}

impl Command for Add {
    type Response = SongId;

    fn to_command(self) -> RawCommand {
        let mut command = RawCommand::new("addid").argument(self.uri);

        if let Some(pos) = self.position {
            command.add_argument(pos).unwrap();
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

    fn to_command(self) -> RawCommand {
        match self.0 {
            Target::Id(id) => RawCommand::new("deleteid").argument(id),
            Target::Range(range) => RawCommand::new("delete").argument(range),
        }
    }
}

/// `move` and `moveid` commands.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Move {
    from: Target,
    to: SongPosition,
}

impl Move {
    /// Move the song with the given ID to the given position.
    pub fn id(id: SongId, to: SongPosition) -> Self {
        Self {
            from: Target::Id(id),
            to,
        }
    }

    /// Move the song at the given position to the given position.
    pub fn position(from: SongPosition, to: SongPosition) -> Self {
        Self {
            from: Target::Range(SongRange::new(from..=from)),
            to,
        }
    }

    /// Move the given range of song positions to the given position.
    pub fn range<R>(range: R, to: SongPosition) -> Self
    where
        R: RangeBounds<SongPosition>,
    {
        Self {
            from: Target::Range(SongRange::new(range)),
            to,
        }
    }
}

impl Command for Move {
    type Response = res::Empty;

    fn to_command(self) -> RawCommand {
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

    fn to_command(self) -> RawCommand {
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

    fn to_command(self) -> RawCommand {
        let mut command = RawCommand::new("list").argument(self.tag);

        if let Some(filter) = self.filter {
            command.add_argument(filter).unwrap();
        }

        if let Some(group_by) = self.group_by {
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

    fn to_command(self) -> RawCommand {
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

    fn to_command(self) -> RawCommand {
        let mut command = RawCommand::new("load").argument(self.name);

        if let Some(range) = self.range {
            command.add_argument(range).unwrap();
        }

        command
    }
}

/// `playlistadd` command.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AddToPlaylist {
    playlist: String,
    song_url: String,
}

impl AddToPlaylist {
    /// Add `song_url` to `playlist`.
    pub fn new(playlist: String, song_url: String) -> Self {
        Self { playlist, song_url }
    }
}

impl Command for AddToPlaylist {
    type Response = res::Empty;

    fn to_command(self) -> RawCommand {
        RawCommand::new("playlistadd")
            .argument(self.playlist)
            .argument(self.song_url)
    }
}

/// `playlistdelete` command.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RemoveFromPlaylist {
    playlist: String,
    song_position: usize,
}

impl RemoveFromPlaylist {
    /// Delete the song at `song_position` from `playlist`.
    pub fn new(playlist: String, song_position: usize) -> Self {
        RemoveFromPlaylist {
            playlist,
            song_position,
        }
    }
}

impl Command for RemoveFromPlaylist {
    type Response = res::Empty;

    fn to_command(self) -> RawCommand {
        RawCommand::new("playlistdelete")
            .argument(self.playlist)
            .argument(self.song_position.to_string())
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

    fn to_command(self) -> RawCommand {
        RawCommand::new("playlistmove")
            .argument(self.playlist)
            .argument(self.from.to_string())
            .argument(self.to.to_string())
    }
}

/// `listallinfo` command.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ListAllSongs {
    directory: String,
}

impl ListAllSongs {
    /// List all songs in the library.
    pub fn new() -> Self {
        Self {
            directory: String::new(),
        }
    }

    /// List all songs beneath the given directory.
    pub fn directory(directory: String) -> Self {
        Self { directory }
    }
}

impl Command for ListAllSongs {
    type Response = Vec<res::Song>;

    fn to_command(self) -> RawCommand {
        let mut command = RawCommand::new("listallinfo");

        if !self.directory.is_empty() {
            command.add_argument(self.directory).unwrap();
        }

        command
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
    }

    #[test]
    fn command_crossfade() {
        assert_eq!(
            Crossfade(Duration::from_secs_f64(2.345)).to_command(),
            RawCommand::new("crossfade").argument("2")
        );
    }

    #[test]
    fn command_volume() {
        assert_eq!(
            SetVolume(150).to_command(),
            RawCommand::new("setvol").argument("100")
        );
    }

    #[test]
    fn command_seek_to() {
        let duration = Duration::from_secs(2);

        assert_eq!(
            SeekTo(SongId(2).into(), duration).to_command(),
            RawCommand::new("seekid")
                .argument(SongId(2))
                .argument(duration)
        );

        assert_eq!(
            SeekTo(SongPosition(2).into(), duration).to_command(),
            RawCommand::new("seek")
                .argument(SongPosition(2))
                .argument(duration)
        );
    }

    #[test]
    fn command_seek() {
        let duration = Duration::from_secs(1);

        assert_eq!(
            Seek(SeekMode::Absolute(duration)).to_command(),
            RawCommand::new("seekcur").argument("1.000")
        );
        assert_eq!(
            Seek(SeekMode::Forward(duration)).to_command(),
            RawCommand::new("seekcur").argument("+1.000")
        );
        assert_eq!(
            Seek(SeekMode::Backward(duration)).to_command(),
            RawCommand::new("seekcur").argument("-1.000")
        );
    }

    #[test]
    fn command_play() {
        assert_eq!(Play::current().to_command(), RawCommand::new("play"));
        assert_eq!(
            Play::song(SongPosition(2)).to_command(),
            RawCommand::new("play").argument(SongPosition(2))
        );
        assert_eq!(
            Play::song(SongId(2)).to_command(),
            RawCommand::new("playid").argument(SongId(2))
        );
    }

    #[test]
    fn command_add() {
        let uri = String::from("foo/bar.mp3");

        assert_eq!(
            Add::uri(uri.clone()).to_command(),
            RawCommand::new("addid").argument(uri.clone())
        );
        assert_eq!(
            Add::uri(uri.clone()).at(5).to_command(),
            RawCommand::new("addid").argument(uri.clone()).argument("5")
        );
    }

    #[test]
    fn command_delete() {
        assert_eq!(
            Delete::id(SongId(2)).to_command(),
            RawCommand::new("deleteid").argument(SongId(2))
        );

        assert_eq!(
            Delete::position(SongPosition(2)).to_command(),
            RawCommand::new("delete").argument("2:3")
        );

        assert_eq!(
            Delete::range(SongPosition(2)..SongPosition(4)).to_command(),
            RawCommand::new("delete").argument("2:4")
        );
    }

    #[test]
    fn command_move() {
        assert_eq!(
            Move::position(SongPosition(2), SongPosition(4)).to_command(),
            RawCommand::new("move").argument("2:3").argument("4")
        );

        assert_eq!(
            Move::id(SongId(2), SongPosition(4)).to_command(),
            RawCommand::new("moveid")
                .argument(SongId(2))
                .argument(SongPosition(4))
        );

        assert_eq!(
            Move::range(SongPosition(3)..SongPosition(5), SongPosition(4)).to_command(),
            RawCommand::new("move")
                .argument("3:5")
                .argument(SongPosition(4))
        );
    }

    #[test]
    fn command_find() {
        let filter = Filter::tag(Tag::Artist, "Foo");

        assert_eq!(
            Find::new(filter.clone()).to_command(),
            RawCommand::new("find").argument(filter.clone())
        );

        assert_eq!(
            Find::new(filter.clone()).window(..3).to_command(),
            RawCommand::new("find")
                .argument(filter.clone())
                .argument("window")
                .argument("0:3"),
        );

        assert_eq!(
            Find::new(filter.clone())
                .window(3..)
                .sort(Tag::Artist)
                .to_command(),
            RawCommand::new("find")
                .argument(filter.clone())
                .argument("sort")
                .argument("Artist")
                .argument("window")
                .argument("3:")
        );
    }

    #[test]
    fn command_listallinfo() {
        assert_eq!(
            ListAllSongs::new().to_command(),
            RawCommand::new("listallinfo")
        );

        assert_eq!(
            ListAllSongs::directory(String::from("foo")).to_command(),
            RawCommand::new("listallinfo").argument("foo")
        );
    }
}
