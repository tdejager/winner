mod client_handler;
mod messages;
mod room;
mod room_communication;
mod types;

use std::env;

use crate::client_handler::ClientHandler;
use crate::room_communication::RoomSubscriber;
use futures::prelude::*;

use tokio::net::TcpListener;



/// Communicate with the room over TCP
async fn tcp_room(room_subscriber: RoomSubscriber) -> anyhow::Result<()> {
    // Parse the arguments, bind the TCP socket we'll be listening to, spin up
    // our worker threads, and start shipping sockets to those worker threads.
    let addr = env::args()
        .nth(1)
        .unwrap_or_else(|| "127.0.0.1:8080".to_string());
    let server = TcpListener::bind(&addr)
        .await
        .expect("Could not open server");

    loop {
        let cloned_subscriber = room_subscriber.clone();
        // Split into a read and a write part
        let (incoming_messages, outgoing_messages) = server.accept().await.unwrap().0.into_split();
        // Spawn a task that handles this connection
        tokio::spawn(async move {
            let client_handler = ClientHandler::new(cloned_subscriber);
            client_handler
                .run(incoming_messages, outgoing_messages)
                .await
                .expect("Error while running the client_handler")
        });
    }
}

#[tokio::main]
async fn main() {
    // Setup a single room
    let (mut room, communication) = room_communication::setup_room();

    // Create a new room, the only one for now
    tokio::spawn(async move { room.run().await });
    tcp_room(communication)
        .await
        .expect("Error while running room task");
}
