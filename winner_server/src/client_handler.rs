use crate::room::{RoomCommunication, WinnerRoomCommunication};
use futures::StreamExt;
use serde_json::Value;
use tokio::net::tcp::{OwnedReadHalf, OwnedWriteHalf};
use tokio_serde::formats::*;
use tokio_util::codec::{FramedRead, LengthDelimitedCodec};

/// Handles the connections to the client
pub struct ClientHandler {
    // Used to setup communication with the room
    room_communication: RoomCommunication,
    // Used to receive json messages
    read_half: OwnedReadHalf,
    // Used to write json messages back to the client
    write_half: OwnedWriteHalf,
}

type MessageStream = tokio_serde::SymmetricallyFramed<
    FramedRead<OwnedReadHalf, LengthDelimitedCodec>,
    Value,
    SymmetricalJson<Value>,
>;

pub async fn wait_on_subscription(
    message_stream: &mut MessageStream,
) -> anyhow::Result<WinnerRoomCommunication> {
    while let Some(msg) = message_stream.next().await {
        // TODO deserialize here and check if we have the correct message type
    }

    unimplemented!();
}

impl ClientHandler {
    /// Returns a new client handler
    pub fn new(
        room_communication: RoomCommunication,
        read_half: OwnedReadHalf,
        write_half: OwnedWriteHalf,
    ) -> Self {
        Self {
            room_communication,
            read_half,
            write_half,
        }
    }

    /// Run the client connection
    pub async fn run(mut self) -> anyhow::Result<()> {
        // Delimit frames using a length header
        let length_delimited = FramedRead::new(self.read_half, LengthDelimitedCodec::new());

        // Deserialize frames
        let mut message_stream =
            MessageStream::new(length_delimited, SymmetricalJson::<Value>::default());

        // Wait for subscription first
        wait_on_subscription(&mut message_stream).await?;

        // We are done
        Ok(())
    }
}
