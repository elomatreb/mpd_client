use crate::{
    commands::Command,
    errors::TypedResponseError,
    raw::{Frame, RawCommandList},
};

/// Types which can be used as a typed command list, using
/// [`Client::command_list`][crate::Client::command_list].
///
/// This is implemented for tuples of [`Command`s][Command] where it returns a tuple of the same
/// size of the responses corresponding to the commands, as well as for a vector of the same
/// command type where it returns a vector of the same length of the responses.
pub trait CommandList {
    /// The responses the list will result in.
    type Response;

    /// The command list that will be sent, or `None` if no commands.
    fn command_list(&self) -> Option<RawCommandList>;

    /// Parse the raw response frames into the proper types.
    fn responses(self, frames: Vec<Frame>) -> Result<Self::Response, TypedResponseError>;
}

/// Arbitrarily long sequence of the same command.
impl<C> CommandList for Vec<C>
where
    C: Command,
{
    type Response = Vec<C::Response>;

    fn command_list(&self) -> Option<RawCommandList> {
        let mut commands = self.iter().map(|c| c.command());
        let mut raw_commands = RawCommandList::new(commands.next()?);
        raw_commands.extend(commands);

        Some(raw_commands)
    }

    fn responses(self, frames: Vec<Frame>) -> Result<Self::Response, TypedResponseError> {
        assert_eq!(self.len(), frames.len());
        let mut out = Vec::with_capacity(self.len());

        for (command, frame) in self.into_iter().zip(frames) {
            out.push(command.response(frame)?);
        }

        Ok(out)
    }
}

macro_rules! impl_command_list_tuple {
    ($first_type:ident, $($further_type:ident => $further_idx:tt),*) => {
        impl<$first_type, $($further_type),*> CommandList for ($first_type, $($further_type),*)
        where
            $first_type: Command,
            $(
                $further_type: Command
            ),*
        {
            type Response = ($first_type::Response, $($further_type::Response),*);

            fn command_list(&self) -> Option<RawCommandList> {
                #[allow(unused_mut)]
                let mut commands = RawCommandList::new(self.0.command());

                $(
                    commands.add(self.$further_idx.command());
                )*

                Some(commands)
            }

            fn responses(self, frames: Vec<Frame>) -> Result<Self::Response, TypedResponseError> {
                let mut frames = frames.into_iter();

                Ok((
                    self.0.response(frames.next().unwrap())?,
                    $(
                        self.$further_idx.response(frames.next().unwrap())?,
                    )*
                ))
            }
        }
    };
}

impl_command_list_tuple!(A,);
impl_command_list_tuple!(A, B => 1);
impl_command_list_tuple!(A, B => 1, C => 2);
impl_command_list_tuple!(A, B => 1, C => 2, D => 3);
impl_command_list_tuple!(A, B => 1, C => 2, D => 3, E => 4);
impl_command_list_tuple!(A, B => 1, C => 2, D => 3, E => 4, F => 5);
impl_command_list_tuple!(A, B => 1, C => 2, D => 3, E => 4, F => 5, G => 6);
impl_command_list_tuple!(A, B => 1, C => 2, D => 3, E => 4, F => 5, G => 6, H => 7);
