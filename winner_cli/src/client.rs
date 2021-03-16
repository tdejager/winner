use std::net::SocketAddr;

use futures::{SinkExt, StreamExt};
use serde_json::Value;
use tokio::net::TcpStream;
use tokio_serde::formats::*;
use tokio_util::codec::{FramedRead, FramedWrite, LengthDelimitedCodec};
use winner_server::messages::{ClientMessages, RoomInitialState, ServerMessages};
use winner_server::types::Winner;
use winner_server::util::{MessageStreamRead, MessageStreamWrite};

/// Struct to talk to the server to using a request response method
pub struct RoomAPI {
    client_messages: MessageStreamRead,
    server_messages: MessageStreamWrite,
}

/// Subscribe to the room and wait for the initial state message
async fn subscribe(
    winner: &Winner,
    writer: &mut MessageStreamWrite,
    reader: &mut MessageStreamRead,
) -> anyhow::Result<RoomInitialState> {
    let subscribe = serde_json::to_value(ClientMessages::EnterRoom(winner.clone()))?;
    // Send subscription request
    writer.send(subscribe).await?;

    while let Some(msg) = reader.next().await {
        let msg = msg?;
        let server_message: ServerMessages = serde_json::from_value(msg)?;

        // Return if we get an initial state
        if let ServerMessages::InitialState(initial_state) = server_message {
            return Ok(initial_state);
        }
    }
    anyhow::bail!("No initial state received")
}

/// Connect to the server, return when a connection is achieved
pub async fn connect_and_subscribe(
    winner: &Winner,
    addr: SocketAddr,
) -> anyhow::Result<(RoomAPI, RoomInitialState)> {
    // Connect to the winner server
    let connection = TcpStream::connect(addr).await?;

    // Split into read and write part
    let (read_half, write_half) = connection.into_split();

    // Create a read and write frame that is delimited by length
    let length_delimited_read = FramedRead::new(read_half, LengthDelimitedCodec::new());
    let length_delimited_write = FramedWrite::new(write_half, LengthDelimitedCodec::new());

    // Create the read part
    let mut read_message_stream =
        MessageStreamRead::new(length_delimited_read, SymmetricalJson::<Value>::default());

    // And a write part, which needs to be wrapped in a mutex, because two tasks will be able to write
    let mut write_message_stream =
        MessageStreamWrite::new(length_delimited_write, SymmetricalJson::<Value>::default());

    // Subscribe and receive initial state
    let initial_state =
        subscribe(winner, &mut write_message_stream, &mut read_message_stream).await?;

    Ok((
        RoomAPI {
            client_messages: read_message_stream,
            server_messages: write_message_stream,
        },
        initial_state,
    ))
}
