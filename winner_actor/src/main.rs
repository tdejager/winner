use actix::Actor;
use log::info;
use tokio::net::TcpListener;
use winner_actor::room::Room;
use winner_actor::tcp_actor::tcp_server;

#[actix::main]
async fn main() {
    env_logger::init();
    info!("Hi, from winner server!");

    // Setup a room
    let room = Room::new("VotingRoom").start();

    // Setup the TCP side
    let tcp_listener = TcpListener::bind("127.0.0.1:8000").await.unwrap();
    info!("Starting server");
    tcp_server(tcp_listener, room).await;
    info!("Shutting down");
}
