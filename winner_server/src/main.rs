use std::env;

use winner_server::tcp_room::run_tcp_loop;

#[tokio::main]
async fn main() {
    // Get the address
    let addr = env::args()
        .nth(1)
        .unwrap_or_else(|| "127.0.0.1:8080".to_string());

    // Run the main loop
    run_tcp_loop(addr).await
}
