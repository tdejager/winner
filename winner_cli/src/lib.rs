use futures::{SinkExt, StreamExt};
use tokio::io::WriteHalf;
use tokio::net::tcp::{OwnedReadHalf, OwnedWriteHalf};
use tokio::{io::ReadHalf, net::TcpStream};
use tokio_util::codec::{Framed, FramedRead, FramedWrite};
use winner_actor::codec::ClientWinnerCodec;
use winner_actor::messages::client::ClientRequest;
use winner_actor::room::RoomState;

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
    state_receiver: tokio::sync::mpsc::Receiver<RoomState>,
    response_receiver: tokio::sync::mpsc::Receiver<anyhow::Result<()>>,
    framed_write: FramedWrite<OwnedWriteHalf, ClientWinnerCodec>,
}

impl ClientAPI {
    pub async fn new(stream: TcpStream) -> Self {
        // Split into read and write interface
        let (read, write) = stream.into_split();
        // Create frames
        let framed_read = FramedRead::new(read, ClientWinnerCodec);
        let framed_write = FramedWrite::new(write, ClientWinnerCodec);
        // Multiplex the reader
        let (response_receiver, state_receiver) = multiplex(framed_read).await;

        Self {
            state_receiver,
            response_receiver,
            framed_write,
        }
    }

    /// Get the next state for the room
    pub async fn next_state(&mut self) -> Option<RoomState> {
        self.state_receiver.recv().await
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
}
