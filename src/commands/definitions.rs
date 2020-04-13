//! Definitions of commands.

use mpd_protocol::Command as RawCommand;

use super::{
    responses::{self as res, Empty},
    Command,
};

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

argless_command!(Next, "next", Empty);
argless_command!(Previous, "previous", Empty);
argless_command!(Stop, "stop", Empty);

argless_command!(Status, "status", res::Status);
