use crate::client_handler::ClientHandler;
use crate::room_communication;
use crate::room_communication::RoomSubscriber;
use tokio::net::TcpListener;

/// Communicate with the room over TCP
pub async fn tcp_room(server: TcpListener, room_subscriber: RoomSubscriber) -> anyhow::Result<()> {
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

/// Create a room and a listener that listens to incoming client connections
pub async fn run_tcp_loop(addr: String) {
    // Setup a single room
    let (mut room, communication) = room_communication::setup_room();
    tokio::spawn(async move { room.run().await });

    // Setup TCP listener
    let server = TcpListener::bind(&addr)
        .await
        .expect("Could not open server");
    // Create a new room, the only one for now
    tcp_room(server, communication)
        .await
        .expect("Error while running room task");
}

#[cfg(test)]
mod tests {
    use std::net::SocketAddr;

    use futures::{Future, SinkExt, StreamExt};
    use tokio::net::TcpListener;
    use tokio::net::TcpStream;

    use crate::util;
    use crate::{
        messages::ClientMessages,
        room_communication,
        types::Winner,
        util::{MessageStreamRead, MessageStreamWrite},
    };

    use super::tcp_room;

    /// Setup TCP test room
    async fn setup_tcp_room() -> (
        std::net::SocketAddr,
        impl Future<Output = anyhow::Result<()>>,
    ) {
        // Setup a single room
        let (mut room, communication) = room_communication::setup_room();

        // Create a new room, the only one for now
        tokio::spawn(async move { room.run().await });
        let server = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("Could not open server");

        let local_addr = server.local_addr().expect("Could not get local address");

        (local_addr, tcp_room(server, communication))
    }

    /// Setup example client
    async fn setup_client(addr: SocketAddr) -> (MessageStreamRead, MessageStreamWrite) {
        let (read_part, write_part) = TcpStream::connect(addr)
            .await
            .expect("Could not connect to server")
            .into_split();

        let reader = util::create_read_stream(read_part);
        let writer = util::create_write_stream(write_part);

        (reader, writer)
    }

    #[tokio::test]
    async fn test_subscribe() {
        let (addr, tcp_room) = setup_tcp_room().await;
        let (mut reader, mut writer) = setup_client(addr).await;

        // Spawn the room
        tokio::spawn(async move { tcp_room.await.expect("Error in room") });

        let winner = Winner("Me".into());

        let enter_message = ClientMessages::EnterRoom(winner);

        // Write enter message
        writer
            .send(serde_json::to_value(enter_message).unwrap())
            .await
            .expect("Could not send subscription message");

        dbg!(reader.next().await);
    }
}
