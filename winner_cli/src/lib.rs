use futures::{SinkExt, StreamExt};
use std::future::Future;
use tokio::io::WriteHalf;
use tokio::net::tcp::{OwnedReadHalf, OwnedWriteHalf};
use tokio::sync::mpsc::Receiver;
use tokio::{io::ReadHalf, net::TcpStream};
use tokio_util::codec::{Framed, FramedRead, FramedWrite};
use winner_actor::codec::ClientWinnerCodec;
use winner_actor::messages::client::{ClientRequest, StartVote};
use winner_actor::room::RoomState;
use winner_server::types::{Story, StoryId, StoryPoints, Winner};

/// Multiplex the read half into server reponse and room state
pub async fn multiplex(
    mut read_half: FramedRead<OwnedReadHalf, ClientWinnerCodec>,
) -> (
    tokio::sync::mpsc::Receiver<anyhow::Result<()>>,
    tokio::sync::mpsc::Receiver<RoomState>,
) {
    let (response_sender, response_receiver) = tokio::sync::mpsc::channel(100);
    let (state_sender, state_receiver) = tokio::sync::mpsc::channel(100);

    // Spawn multiplexer task
    tokio::spawn(async move {
        while let Some(item) = read_half.next().await {
            match item {
                Ok(response) => match response {
                    winner_actor::messages::server::ServerResponse::Ok => {
                        response_sender
                            .send(Ok(()))
                            .await
                            .expect("Error sending over channel");
                    }
                    winner_actor::messages::server::ServerResponse::Err(e) => {
                        response_sender
                            .send(Err(anyhow::anyhow!(e)))
                            .await
                            .expect("Could not send over channel");
                    }
                    winner_actor::messages::server::ServerResponse::State(state) => {
                        state_sender
                            .send(state)
                            .await
                            .expect("Error sending over channel");
                    }
                    _ => {}
                },
                Err(_) => break,
            }
        }
    });

    (response_receiver, state_receiver)
}

pub struct ClientAPI {
    response_receiver: tokio::sync::mpsc::Receiver<anyhow::Result<()>>,
    framed_write: FramedWrite<OwnedWriteHalf, ClientWinnerCodec>,
    winner: Winner,
}

pub async fn create_client(stream: TcpStream, winner: Winner) -> (ClientAPI, Receiver<RoomState>) {
    // Split into read and write interface
    let (read_half, write_half) = stream.into_split();
    // Create frames
    let framed_read = FramedRead::new(read_half, ClientWinnerCodec);
    let framed_write = FramedWrite::new(write_half, ClientWinnerCodec);
    // Multiplex the reader
    let (response_receiver, state_receiver) = multiplex(framed_read).await;
    (
        ClientAPI::new(framed_write, response_receiver, winner),
        state_receiver,
    )
}

impl ClientAPI {
    fn new(
        framed_write: FramedWrite<OwnedWriteHalf, ClientWinnerCodec>,
        response_receiver: tokio::sync::mpsc::Receiver<anyhow::Result<()>>,
        winner: Winner,
    ) -> Self {
        Self {
            response_receiver,
            framed_write,
            winner,
        }
    }

    /// Send the client request and wait for a response
    async fn send_request(&mut self, request: ClientRequest) -> anyhow::Result<()> {
        self.framed_write.send(request).await?;

        // Receive the response
        // If no Option is none map to result
        // Fail if response is an error
        self.response_receiver
            .recv()
            .await
            .ok_or_else(|| anyhow::anyhow!("Channel closed"))??;
        Ok(())
    }

    /// Enter the actual room
    pub async fn enter_room(&mut self) -> anyhow::Result<()> {
        self.send_request(ClientRequest::Enter(self.winner.clone()))
            .await
    }

    /// Vote on a story
    pub async fn vote(&mut self, story: StoryId, points: StoryPoints) -> anyhow::Result<()> {
        self.send_request(ClientRequest::Vote(self.winner.clone(), story, points))
            .await
    }

    /// Start a new vote
    pub async fn start_vote(&mut self, story: Story) -> anyhow::Result<()> {
        self.send_request(ClientRequest::StartVote(story)).await
    }

    /// Restart a vote
    pub async fn restart_vote(&mut self) -> anyhow::Result<()> {
        self.send_request(ClientRequest::RestartVote).await
    }
}
