use futures::Future;
use tokio::net::TcpListener;
use winner_cli::client;
use winner_server::{
    room_communication, tcp_room::tcp_room, types::Story, types::StoryId, types::StoryPoints,
    types::Winner,
};

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
async fn client_subscribe() {
    let (address, tcp_room) = setup_tcp_room().await;
    tokio::spawn(async move { tcp_room.await.expect("Error while running room") });
    let winner = Winner("Me".into());

    // Try to connect
    client::connect_and_subscribe(&winner, address)
        .await
        .expect("Could not connect to server");
}

#[tokio::test]
async fn client_subscribe2() {
    let (address, tcp_room) = setup_tcp_room().await;
    tokio::spawn(async move { tcp_room.await.expect("Error while running room") });
    let winner = Winner("Me".into());
    let rival = Winner("Rival".into());

    // Try to connect
    client::connect_and_subscribe(&winner, address)
        .await
        .expect("Could not connect to server");

    // Connect again
    client::connect_and_subscribe(&rival, address)
        .await
        .expect("Could not connect to server");
}

#[tokio::test]
async fn client_vote() {
    let (address, tcp_room) = setup_tcp_room().await;
    tokio::spawn(async move { tcp_room.await.expect("Error while running room") });
    let winner = Winner("Me".into());
    let rival = Winner("Rival".into());

    // Try to connect
    let (mut winner_api, _updater_winner, ..) = client::connect_and_subscribe(&winner, address)
        .await
        .expect("Could not connect to server");

    // Connect again
    let (mut rival_api, _updater_rival, ..) = client::connect_and_subscribe(&rival, address)
        .await
        .expect("Could not connect to server");

    let story = Story::new(StoryId(1), "Cool story");
    winner_api
        .start_vote(&story)
        .await
        .expect("Could not start vote");
    winner_api
        .vote(&story, StoryPoints::ONE)
        .await
        .expect("Winner could not vote");
    rival_api
        .vote(&story, StoryPoints::TWO)
        .await
        .expect("Rival could not vote");
}
