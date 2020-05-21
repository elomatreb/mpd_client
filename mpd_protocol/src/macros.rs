/// Conveniently generate a [`CommandList`].
///
/// ```
/// use mpd_protocol::{command_list, Command, CommandList};
///
/// command_list![
///     Command::new("status"),
///     Command::new("pause").argument("1")
/// ];
///
/// // Equivalent to:
///
/// {
///     let mut command_list = CommandList::new(
///         Command::new("status")
///     );
///     command_list.add(
///         Command::new("pause").argument("1")
///     );
///     command_list
/// };
/// ```
///
/// [`CommandList`]: crate::command::CommandList
#[macro_export]
macro_rules! command_list {
    ($first:expr) => {
        command_list!($first,)
    };
    ($first:expr, $( $tail:expr ),*) => {
        {
            #[allow(unused_mut)]
            let mut list = $crate::CommandList::new($first);

            $(
                list.add($tail);
            )*

            list
        }
    };
}
