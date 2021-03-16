use futures::Future;
use tokio::net::TcpListener;
use winner_cli::client;
use winner_server::{room_communication, tcp_room::tcp_room, types::Winner};

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

#[tokio::test]
async fn client_subcribe() {
    let (address, tcp_room) = setup_tcp_room().await;
    tokio::spawn(async move { tcp_room.await.expect("Error while running room") });
    let winner = Winner("Me".into());

    // Try to connect
    client::connect_and_subscribe(&winner, address).await.expect("Could not connect to server");
}
