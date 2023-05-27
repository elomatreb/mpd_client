use std::error::Error;

use mpd_client::{
    client::{ConnectionEvent, Subsystem},
    commands, Client,
};
use tokio::net::TcpStream;

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn Error>> {
    tracing_subscriber::fmt().init();

    // Connect via TCP
    let connection = TcpStream::connect("localhost:6600").await?;
    // Or through a Unix socket
    // let connection = UnixStream::connect("/run/user/1000/mpd").await?;

    // The client is used to issue commands, and state_changes is an async stream of state change
    // notifications
    let (client, mut state_changes) = Client::connect(connection).await?;

    'outer: loop {
        match client.command(commands::CurrentSong).await? {
            Some(song_in_queue) => {
                println!(
                    "\"{}\" by \"{}\"",
                    song_in_queue.song.title().unwrap_or(""),
                    song_in_queue.song.artists().join(", "),
                );
            }
            None => println!("(none)"),
        }

        loop {
            // wait for a state change notification in the player subsystem, which indicates a song
            // change among other things
            match state_changes.next().await {
                Some(ConnectionEvent::SubsystemChange(Subsystem::Player)) => break, /* something relevant changed */
                Some(ConnectionEvent::SubsystemChange(_)) => continue, /* something changed but we don't care */
                _ => break 'outer, // connection was closed by the server
            }
        }
    }

    Ok(())
}
