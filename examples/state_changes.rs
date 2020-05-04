use std::error::Error;
use tokio::stream::StreamExt; // for .next()
use tracing_subscriber::{EnvFilter, FmtSubscriber};

use mpd_client::{
    commands::{self, responses::Tag},
    Client, Subsystem,
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    FmtSubscriber::builder()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    // The client is used to issue commands, and state_changes is an async stream of state change
    // notifications
    let (client, mut state_changes) = Client::connect_to("localhost:6600").await?;

    // You can also connect to Unix sockets
    // let (client, mut state_changes) = Client::connect_unix("/run/user/1000/mpd").await?;

    // Get the song playing right as we connect
    print_current_song(&client).await?;

    // Wait for state change notifications being emitted by MPD
    while let Some(subsys) = state_changes.next().await {
        let subsys = subsys?;

        if subsys == Subsystem::Player {
            print_current_song(&client).await?;
        }
    }

    Ok(())
}

async fn print_current_song(client: &Client) -> Result<(), Box<dyn Error>> {
    match client.command(commands::CurrentSong).await? {
        Some(song) => {
            println!(
                "\"{}\" by \"{}\"",
                song.tags
                    .get(&Tag::Title)
                    .map(|values| values.join(", "))
                    .unwrap_or_else(|| "(none)".to_string()),
                song.tags
                    .get(&Tag::Artist)
                    .map(|values| values.join(", "))
                    .unwrap_or_else(|| "(none)".to_string()),
            );
        }
        None => println!("(none)"),
    }

    Ok(())
}
