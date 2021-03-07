#![allow(dead_code)]
use crate::messages::StateChange;
use crate::messages::{ClientMessages, ServerMessages};
use crate::room_communication::{RoomAPI, RoomInitialState, RoomStateUpdater, RoomSubscriber};
use futures::{SinkExt, StreamExt};
use serde_json::Value;
use std::sync::Arc;
use tokio::net::tcp::{OwnedReadHalf, OwnedWriteHalf};
use tokio::sync::Mutex;
use tokio_serde::formats::*;
use tokio_util::codec::{FramedRead, FramedWrite, LengthDelimitedCodec};

/// Handles the connections to the client
pub struct ClientHandler {
    // Used to setup communication with the room
    room_communication: RoomSubscriber,
}

/// This represents a message stream
type MessageStreamRead = tokio_serde::SymmetricallyFramed<
    FramedRead<OwnedReadHalf, LengthDelimitedCodec>,
    Value,
    SymmetricalJson<Value>,
>;

/// This represents a message stream
type MessageStreamWrite = tokio_serde::SymmetricallyFramed<
    FramedWrite<OwnedWriteHalf, LengthDelimitedCodec>,
    Value,
    SymmetricalJson<Value>,
>;

impl ClientHandler {
    /// Returns a new client handler
    pub fn new(room_subscriber: RoomSubscriber) -> Self {
        Self { room_communication: room_subscriber }
    }

    /// Wait for subscription method and then subscribe to the room
    pub async fn wait_on_subscription(
        &self,
        message_stream: &mut MessageStreamRead,
    ) -> anyhow::Result<(RoomAPI, RoomStateUpdater, RoomInitialState)> {
        while let Some(msg) = message_stream.next().await {
            let msg = msg?;
            let client_message: ClientMessages = serde_json::from_value(msg)?;

            // See if this is a subscription message
            if let ClientMessages::RoomStateChange(room_message) = client_message {
                let (winner, state_change) = room_message;
                if state_change == StateChange::Enter {
                    // Subscribe to room
                    return self.room_communication.subscribe(&winner).await;
                }
            }
        }

        // Stream closed while no message was received
        anyhow::bail!("No subscription received")
    }

    /// Receives client messages, passes them to the server and send back the response
    async fn message_handler(
        room_api: RoomAPI,
        mut read_message_stream: MessageStreamRead,
        write_message_stream: Arc<Mutex<MessageStreamWrite>>,
    ) -> anyhow::Result<()> {
        // Receive messages
        while let Some(msg) = read_message_stream.next().await {
            let msg = msg?;
            let client_message: ClientMessages = serde_json::from_value(msg)?;

            // Dispatch these correctly
            let result = match client_message {
                ClientMessages::RoomStateChange((_winner, change)) => match change {
                    // Explicit leave message received
                    StateChange::Leave => return room_api.leave_room().await,
                    StateChange::Leader => {
                        unimplemented!()
                    }
                    // This cannot be sent again
                    StateChange::Enter => Err(anyhow::anyhow!("Incorrect message")),
                },
                ClientMessages::AcknowledgeLeader(_) => {
                    unimplemented!()
                }
                ClientMessages::StartVote(story) => room_api.start_vote(&story).await,
                ClientMessages::Vote((_winner, story, points)) => {
                    room_api.cast_vote(&story, points).await
                }
                ClientMessages::FightResolved => {
                    unimplemented!()
                }
            };

            // Convert the response
            let response = result.map_or_else(
                |e| ServerMessages::ServerErr(format!("{}", e)),
                |_| ServerMessages::ServerOk(),
            );

            // And handle the result
            write_message_stream
                .lock()
                .await
                .send(serde_json::to_value(response)?)
                .await?;
        }

        // If we come here, out stream is closed but we still need to tell the room we have left
        room_api.leave_room().await;
        Ok(())
    }

    /// Run the client connection
    pub async fn run(
        &self,
        read_half: OwnedReadHalf,
        write_half: OwnedWriteHalf,
    ) -> anyhow::Result<()> {
        // Create a read and write frame that is delimited by length
        let length_delimited_read = FramedRead::new(read_half, LengthDelimitedCodec::new());
        let length_delimited_write = FramedWrite::new(write_half, LengthDelimitedCodec::new());

        // Create a read part
        let mut read_message_stream =
            MessageStreamRead::new(length_delimited_read, SymmetricalJson::<Value>::default());

        // Wait for subscription first, this is the first message we should receive
        let (room_api, state_updater, _initial_state) =
            self.wait_on_subscription(&mut read_message_stream).await?;

        // And a write part, which needs to be wrapped in a mutex, because two tasks will be able to write
        let write_message_stream = Arc::new(Mutex::new(MessageStreamWrite::new(
            length_delimited_write,
            SymmetricalJson::<Value>::default(),
        )));

        // TODO still requires a part that handles the server updates
        // TODO change this to a join
        tokio::try_join!(ClientHandler::message_handler(
            room_api,
            read_message_stream,
            write_message_stream.clone()
        ))?;

        // We are done
        Ok(())
    }
}
