use crate::commands::{
    responses::{Response, TypedResponseError},
    Command,
};
use crate::raw::{Frame, RawCommandList};
use crate::sealed;

/// Types which can be used as a typed command list.
///
/// This is implemented for tuples of [`Command`s][Command] where it returns a tuple of the same
/// size of the responses corresponding to the commands, as well as for a vector of the same
/// command type where it returns a vector of the same length of the responses.
pub trait CommandList: sealed::Sealed {
    /// The responses the list will result in.
    type Response;

    /// Generate the command list that will be sent, or `None` if no commands.
    #[doc(hidden)]
    fn to_raw_command_list(self) -> Option<RawCommandList>;

    /// Parse the raw response frames into the proper types.
    #[doc(hidden)]
    fn parse_responses(frames: Vec<Frame>) -> Result<Self::Response, TypedResponseError>;
}

impl<C: Command> sealed::Sealed for Vec<C> {}

/// Arbitrarily long sequence of the same command.
impl<C> CommandList for Vec<C>
where
    C: Command,
{
    type Response = Vec<C::Response>;

    fn to_raw_command_list(self) -> Option<RawCommandList> {
        let mut commands = self.into_iter().map(|c| c.to_command());

        let mut raw_commands = RawCommandList::new(commands.next()?);

        raw_commands.extend(commands);

        Some(raw_commands)
    }

    fn parse_responses(frames: Vec<Frame>) -> Result<Self::Response, TypedResponseError> {
        let frames = frames.into_iter();
        let (lower, _) = frames.size_hint();
        let mut out = Vec::with_capacity(lower);

        for frame in frames {
            out.push(<C::Response>::from_frame(frame)?);
        }

        Ok(out)
    }
}

macro_rules! impl_command_list_tuple {
    ($first_type:ident, $($further_type:ident => $further_idx:tt),*) => {
        impl<$first_type, $($further_type),*> sealed::Sealed for ($first_type, $($further_type),*)
        where
            $first_type: Command,
            $(
                $further_type: Command
            ),*
        {}

        impl<$first_type, $($further_type),*> CommandList for ($first_type, $($further_type),*)
        where
            $first_type: Command,
            $(
                $further_type: Command
            ),*
        {
            type Response = ($first_type::Response, $($further_type::Response),*);

            fn to_raw_command_list(self) -> Option<RawCommandList> {
                #[allow(unused_mut)]
                let mut commands = RawCommandList::new(self.0.to_command());

                $(
                    commands.add(self.$further_idx.to_command());
                )*

                Some(commands)
            }

            fn parse_responses(frames: Vec<Frame>) -> Result<Self::Response, TypedResponseError> {
                let mut frames = frames.into_iter();

                Ok((
                    <$first_type::Response>::from_frame(frames.next().unwrap())?,
                    $(
                        <$further_type::Response>::from_frame(frames.next().unwrap())?,
                    )*
                ))
            }
        }
    };
}

impl_command_list_tuple!(A,);
impl_command_list_tuple!(A, B => 1);
impl_command_list_tuple!(A, B => 1, C => 2);
impl_command_list_tuple!(A, B => 1, C => 2, D => 3, E => 4);
impl_command_list_tuple!(A, B => 1, C => 2, D => 3, E => 4, F => 5);
impl_command_list_tuple!(A, B => 1, C => 2, D => 3, E => 4, F => 5, G => 6);
impl_command_list_tuple!(A, B => 1, C => 2, D => 3, E => 4, F => 5, G => 6, H => 7);
