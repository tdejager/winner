use std::env;
use winner_cli::client;
use winner_server::types::Winner;

#[tokio::main]
async fn main() {
    // Get the address
    let addr = env::args()
        .nth(1)
        .unwrap_or_else(|| "127.0.0.1:8080".to_string());

    let winner = Winner("Me".into());
    let _ = client::connect_and_subscribe(&winner, addr.parse().unwrap());
    println!("Hello, world!");
}
