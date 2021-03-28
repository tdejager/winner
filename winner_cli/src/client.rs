use std::net::SocketAddr;

use crate::room_api::ClientRoomAPI;
use futures::{SinkExt, StreamExt};
use serde_json::Value;
use tokio::{net::TcpStream, sync::mpsc};
use tokio_serde::formats::*;
use tokio_util::codec::{FramedRead, FramedWrite, LengthDelimitedCodec};
use winner_server::messages::{ClientMessages, RoomInitialState, ServerMessages};
use winner_server::types::Winner;
use winner_server::util::{MessageStreamRead, MessageStreamWrite};

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

/// Routes messages to correct receiver
pub async fn router(
    response_sender: mpsc::Sender<ServerMessages>,
    update_sender: mpsc::Sender<ServerMessages>,
    mut read_stream: MessageStreamRead,
) -> anyhow::Result<()> {
    // Read and route to the correct responder
    while let Some(msg) = read_stream.next().await {
        let msg = msg?;
        // Decode message
        let msg: ServerMessages = serde_json::from_value(msg)?;

        dbg!(&msg);
        match msg {
            // This was a request response type message, route to response sender
            ServerMessages::ServerOk() => response_sender.send(msg).await?,
            ServerMessages::ServerErr(_) => response_sender.send(msg).await?,
            // This was an update, route to update sender
            _ => update_sender.send(msg).await?,
        }
    }
    Ok(())
}

/// Connect to the server, return when a connection is achieved
pub async fn connect_and_subscribe(
    winner: &Winner,
    addr: SocketAddr,
) -> anyhow::Result<(
    ClientRoomAPI,
    mpsc::Receiver<ServerMessages>,
    RoomInitialState,
)> {
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

    let (response_sender, response_receiver) = mpsc::channel(100);
    let (update_sender, update_receiver) = mpsc::channel(100);

    // Subscribe and receive initial state
    let initial_state =
        subscribe(winner, &mut write_message_stream, &mut read_message_stream).await?;

    // Spawn router task
    tokio::spawn(async move {
        router(response_sender, update_sender, read_message_stream)
            .await
            .expect("Router failed")
    });

    Ok((
        ClientRoomAPI::new(winner, write_message_stream, response_receiver),
        update_receiver,
        initial_state,
    ))
}
