use crate::tcp_actor::tcp_server;
use actix::Actor;
use log::info;
use tokio::net::TcpListener;

mod codec;
mod messages;
mod room;
mod tcp_actor;

#[actix::main]
async fn main() {
    env_logger::init();

    // Setup a room
    let room = room::Room::new("VotingRoom").start();

    // Setup the TCP side
    let tcp_listener = TcpListener::bind("127.0.0.1:8000").await.unwrap();
    info!("Starting server");
    tcp_server(tcp_listener, room).await;
    info!("Shutting down");
}
