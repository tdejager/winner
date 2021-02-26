mod messages;
mod room;
mod types;

use std::env;

use futures::prelude::*;
use room::RoomCommunication;
use serde_json::Value;
use tokio::net::TcpListener;
use tokio_serde::formats::*;
use tokio_util::codec::{FramedRead, LengthDelimitedCodec};

/// Communicate with the room over TCP
async fn tcp_room(communication: RoomCommunication) {
    // Parse the arguments, bind the TCP socket we'll be listening to, spin up
    // our worker threads, and start shipping sockets to those worker threads.
    let addr = env::args()
        .nth(1)
        .unwrap_or_else(|| "127.0.0.1:8080".to_string());
    let server = TcpListener::bind(&addr)
        .await
        .expect("Could not open server");

    loop {
        let (socket, _) = server.accept().await.unwrap();

        // Delimit frames using a length header
        let length_delimited = FramedRead::new(socket, LengthDelimitedCodec::new());

        // Deserialize frames
        let mut deserialized = tokio_serde::SymmetricallyFramed::new(
            length_delimited,
            SymmetricalJson::<Value>::default(),
        );

        // Spawn a task that prints all received messages to STDOUT
        tokio::spawn(async move {
            while let Some(msg) = deserialized.try_next().await.unwrap() {
                // Just print for now
                println!("GOT: {:?}", msg);
            }
        });
    }
}

#[tokio::main]
async fn main() {
    // Setup a single room
    let (mut room, communication) = room::setup_room();

    // Create a new room
    tokio::spawn(async move { room.run().await });

    // TODO: create a server here that accepts a tcp connection, should block here
    // TODO: Handle subscribes from the tcp and communicate with the room
    // TODO: Initially just create something that subscribes
    tcp_room(communication).await;
}
