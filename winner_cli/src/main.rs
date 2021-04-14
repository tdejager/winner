use std::env;
use winner_cli;
use winner_server::types::Winner;

#[tokio::main]
async fn main() {
    // Get the address
    let addr = env::args()
        .nth(1)
        .unwrap_or_else(|| "127.0.0.1:8080".to_string());

    let tcp_stream = tokio::net::TcpStream::connect(addr)
        .await
        .expect("Could not connect to server");

    // Connect the client
    let _client = winner_cli::ClientAPI::new(tcp_stream, Winner("Me".to_string())).await;
    
    // Todo actually do something
}
