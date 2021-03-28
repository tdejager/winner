use futures::SinkExt;
use thiserror::Error;
use tokio::sync::mpsc::Receiver;
use tokio::time::timeout;
use winner_server::types::{StoryPoints, Winner};
use winner_server::{
    messages::ClientMessages, messages::ServerMessages, types::Story, util::MessageStreamWrite,
};

/// Struct to talk to the server using a request response method
/// as well as receive updates
pub struct ClientRoomAPI {
    winner: Winner,
    client_messages: MessageStreamWrite,
    // Receives request/response room messages
    response_receiver: Receiver<ServerMessages>,
}

const TIMEOUT: u64 = 1;

#[derive(Error, Debug)]
pub enum ClientRoomError {
    #[error("Error while communicating with server")]
    ServerError,
    #[error("Invalid request error {0}")]
    InvalidRequest(String),
    #[error("Received an invalid response")]
    InvalidResponse,
}

impl ClientRoomAPI {
    pub fn new(
        winner: &Winner,
        client_messages: MessageStreamWrite,
        response_receiver: Receiver<ServerMessages>,
    ) -> Self {
        Self {
            winner: winner.clone(),
            client_messages,
            response_receiver,
        }
    }
    /// Send a message to the room, expects a response on the channel
    async fn send_request(&mut self, message: ClientMessages) -> anyhow::Result<()> {
        // Serialize message
        self.client_messages
            .send(serde_json::to_value(message)?)
            .await?;

        // Wait for response, with a TIMEOUT
        let response = timeout(
            std::time::Duration::from_secs(TIMEOUT),
            self.response_receiver.recv(),
        )
        .await?
        .ok_or(ClientRoomError::ServerError)?;

        // Match the response from the room
        match response {
            ServerMessages::ServerOk() => Ok(()),
            ServerMessages::ServerErr(e) => Err(ClientRoomError::InvalidRequest(e))?,
            _ => Err(ClientRoomError::InvalidResponse)?,
        }
    }

    /// Start a new vote
    pub async fn start_vote(&mut self, story: &Story) -> anyhow::Result<()> {
        Ok(self
            .send_request(ClientMessages::StartVote(story.clone()))
            .await?)
    }

    /// Cast a vote
    pub async fn vote(&mut self, story: &Story, points: StoryPoints) -> anyhow::Result<()> {
        Ok(self
            .send_request(ClientMessages::Vote((
                self.winner.clone(),
                story.clone(),
                points.clone(),
            )))
            .await?)
    }
    /// Resolve actual fight
    pub async fn resolve_fight(&mut self) -> anyhow::Result<()> {
        Ok(self.send_request(ClientMessages::FightResolved).await?)
    }
}
