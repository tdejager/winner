mod client_handler;
mod messages;
mod room;
mod room_communication;
mod types;

use std::env;

use crate::client_handler::ClientHandler;
use crate::room_communication::RoomSubscriber;

use tokio::net::TcpListener;

/// Communicate with the room over TCP
async fn tcp_room(server: TcpListener, room_subscriber: RoomSubscriber) -> anyhow::Result<()> {
    // Parse the arguments, bind the TCP socket we'll be listening to, spin up
    // our worker threads, and start shipping sockets to those worker threads.

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
    let addr = env::args()
        .nth(1)
        .unwrap_or_else(|| "127.0.0.1:8080".to_string());

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

    use futures::Future;
    use tokio::net::TcpListener;
    use tokio::net::TcpStream;

    use crate::{room_communication, tcp_room};

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

    async fn setup_client(addr: SocketAddr) -> TcpStream {
        TcpStream::connect(addr)
            .await
            .expect("Could not connect to server")
    }

    #[tokio::test]
    async fn test_subscribe() {
        let (addr, tcp_room) = setup_tcp_room().await;
        let client = setup_client(addr).await;

        // TODO send subscribe message here and await correct response
    }
}
