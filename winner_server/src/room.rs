use std::collections::HashSet;

use futures::future::FutureExt;
use tokio::sync::broadcast::{channel, Receiver, Sender};
use tokio::sync::mpsc;
use tokio::sync::oneshot;

use crate::{
    messages::{ClientMessages, ServerMessages},
    types::Winner,
};

const CHANNEL_SIZE: usize = 100;

/// The state that a Room can be in
enum RoomState {
    /// Waiting for a vote
    Idle,
    /// Actually voting
    Voting,
}

#[derive(Debug)]
pub struct SubscriptionRequest {
    pub winner: Winner,
    pub response: oneshot::Sender<SubscriptionResponse>,
}

#[derive(Debug)]
pub struct SubscriptionData {
    pub message_receive: Receiver<ServerMessages>,
    pub winners: Vec<Winner>,
}

#[derive(Debug)]
pub enum SubscriptionResponse {
    Ok(SubscriptionData),
    WinnerExists,
}

/// Implementation of a room
pub struct Room {
    current_state: RoomState,
    /// Current paricipants in the room
    paricipants: HashSet<Winner>,
    /// Incoming message that need to be processed
    incoming: Receiver<ClientMessages>,
    /// Outgoing message that will be processed by the clients
    outgoing: Sender<ServerMessages>,
    /// Receives subscription request
    subscription_receiver: mpsc::Receiver<SubscriptionRequest>,
}

impl Room {
    /// Create a new room
    pub fn new(
        subscription_receiver: mpsc::Receiver<SubscriptionRequest>,
        incoming: Receiver<ClientMessages>,
        outgoing: Sender<ServerMessages>,
    ) -> Self {
        Room {
            paricipants: HashSet::new(),
            current_state: RoomState::Idle,
            incoming,
            outgoing,
            subscription_receiver,
        }
    }

    /// Subscribe to this room
    fn subscribe(&mut self, subscription_request: &SubscriptionRequest) -> SubscriptionResponse {
        if !self.paricipants.contains(&subscription_request.winner) {
            self.paricipants.insert(subscription_request.winner.clone());
            // Get all participants
            let winners = self
                .paricipants
                .iter()
                .map(|winner| winner.clone())
                .collect::<Vec<Winner>>();
            SubscriptionResponse::Ok(SubscriptionData {
                message_receive: self.outgoing.subscribe(),
                winners,
            })
        } else {
            SubscriptionResponse::WinnerExists
        }
    }

    /// Main loop of the room that is running
    pub async fn run(&mut self) {
        loop {
            // Run different states for the room
            match self.current_state {
                RoomState::Idle => self.idle_loop().await,
                RoomState::Voting => {}
            }

            // Wait to yield for now
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    }

    async fn idle_loop(&mut self) {
        // Check for new subscriptions
        // subscribe if possible
        if let Some(Some(req)) = self.subscription_receiver.recv().now_or_never() {
            println!("Received subscription");
            let response = self.subscribe(&req);
            println!("Sending response");
            req.response
                .send(response)
                .expect("Could not send subscription response");
            println!("Response sent");
        }
    }
}

/// Contains all the channels to communicate with a Room
pub struct RoomCommunication {
    /// Sender to which can be cloned to create server messages
    pub client_sender: Sender<ClientMessages>,
    /// Sender to try to get a subscription to the room
    pub subscription_sender: mpsc::Sender<SubscriptionRequest>,
}

impl RoomCommunication {
    pub fn new(
        client_sender: Sender<ClientMessages>,
        subscription_sender: mpsc::Sender<SubscriptionRequest>,
    ) -> Self {
        Self {
            client_sender,
            subscription_sender,
        }
    }

    pub async fn subscribe(&self, winner: &Winner) -> SubscriptionResponse {
        let (tx, rx) = oneshot::channel();
        self.subscription_sender
            .send(SubscriptionRequest {
                winner: winner.clone(),
                response: tx,
            })
            .await
            .expect("Could not send subscription request");
        println!("Waiting on response");
        rx.await.expect("Could not get subscription response")
    }
}

pub fn setup_room() -> (Room, RoomCommunication) {
    let (subscription_sender, subscription_receiver) = mpsc::channel(CHANNEL_SIZE);
    let (server_sender, _) = channel(CHANNEL_SIZE);
    let (client_sender, client_receiver) = channel(CHANNEL_SIZE);
    (
        Room::new(subscription_receiver, client_receiver, server_sender),
        RoomCommunication::new(client_sender, subscription_sender),
    )
}

#[cfg(test)]
mod tests {
    use crate::types::Winner;

    use super::setup_room;

    #[tokio::test]
    pub async fn test_subscription() {
        let winner = Winner("Me".into());
        let rival = Winner("Rival".into());

        let (mut room, communcation) = setup_room();

        // Run the room
        tokio::spawn(async move { room.run().await });

        // Try to create a subscription
        match communcation.subscribe(&winner).await {
            super::SubscriptionResponse::Ok(_) => assert!(true),
            super::SubscriptionResponse::WinnerExists => assert!(false),
        }

        // Try to create another, subscription
        match communcation.subscribe(&rival).await {
            super::SubscriptionResponse::Ok(data) => assert!(data.winners.contains(&rival)),
            super::SubscriptionResponse::WinnerExists => assert!(false),
        }
    }
}
