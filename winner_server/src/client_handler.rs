use crate::messages::{ClientMessages, ServerMessages};
use crate::messages::RoomInitialState;
use crate::room_communication::{RoomAPI, RoomStateUpdater, RoomSubscriber};
use crate::util::*;
use futures::{SinkExt, StreamExt};
use tokio::net::tcp::{OwnedReadHalf, OwnedWriteHalf};

/// Handles the connections to the client
pub struct ClientHandler {
    // Used to setup communication with the room
    room_communication: RoomSubscriber,
}


/// Receives client messages, passes them to the server and send back the response
async fn message_handler(
    room_api: RoomAPI,
    mut read_message_stream: MessageStreamRead,
    write_message_stream: SharedStreamWrite,
) -> anyhow::Result<()> {
    // Receive messages
    while let Some(msg) = read_message_stream.next().await {
        let msg = msg?;
        let client_message: ClientMessages = serde_json::from_value(msg)?;

        // Dispatch these correctly
        let result = match client_message {
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
            // Ignore the other messages
            _ => Ok(()),
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
    room_api.leave_room().await?;
    Ok(())
}

/// Receives room messages and forwards these to the client
async fn room_updates(
    mut state_update: RoomStateUpdater,
    write_message_stream: SharedStreamWrite,
) -> anyhow::Result<()> {
    loop {
        let room_message = state_update.receive_message().await?;

        write_message_stream
            .lock()
            .await
            .send(serde_json::to_value(room_message)?)
            .await?;
    }
}

/// Write the intial state
async fn write_initial_state(
    write_message_stream: &SharedStreamWrite,
    initial_state: RoomInitialState,
) -> anyhow::Result<()> {
    write_message_stream
        .lock()
        .await
        .send(serde_json::to_value(ServerMessages::InitialState(
            initial_state,
        ))?)
        .await?;
    Ok(())
}

impl ClientHandler {
    /// Returns a new client handler
    pub fn new(room_subscriber: RoomSubscriber) -> Self {
        Self {
            room_communication: room_subscriber,
        }
    }

    /// Wait for subscription method and then subscribe to the room
    pub async fn wait_on_subscription(
        &self,
        message_stream: &mut MessageStreamRead,
    ) -> anyhow::Result<(RoomAPI, RoomStateUpdater, RoomInitialState)> {
        // Wait for subscription method
        while let Some(msg) = message_stream.next().await {
            let msg = msg?;
            let client_message: ClientMessages = serde_json::from_value(msg)?;

            // See if this is a subscription message
            if let ClientMessages::EnterRoom(winner) = client_message {
                // Subscribe to room
                return self.room_communication.subscribe(&winner).await;
            }
        }

        // Stream closed while no message was received
        anyhow::bail!("No subscription received")
    }

    /// Run the client connection
    pub async fn run(
        &self,
        read_half: OwnedReadHalf,
        write_half: OwnedWriteHalf,
    ) -> anyhow::Result<()> {

        // Create a read part
        let mut read_message_stream = create_read_stream(read_half);
        // Wait for subscription first, this is the first message we should receive
        let (room_api, state_updater, initial_state) =
            self.wait_on_subscription(&mut read_message_stream).await?;

        // And a write part, which needs to be wrapped in a mutex, because two tasks will be able to write
        let write_message_stream = create_shared_write_stream(write_half);

        // Write the initial state back to the client
        write_initial_state(&write_message_stream, initial_state).await?;

        // This will run the rest of the time
        tokio::try_join!(
            // Handle client messages forward them to the room
            message_handler(room_api, read_message_stream, write_message_stream.clone()),
            // Handle room messages forward these to the client
            room_updates(state_updater, write_message_stream)
        )?;

        // We are done
        Ok(())
    }
}
