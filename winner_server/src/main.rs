mod messages;
mod room;
mod types;

use room::RoomCommunication;

/// Communicate with the room over TCP
async fn tcp_room(communication: RoomCommunication) {}

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
