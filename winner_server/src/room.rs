#![allow(dead_code)]
use std::collections::HashSet;

use futures::future::FutureExt;
use tokio::sync::broadcast::{channel, error::SendError, Receiver, Sender};
use tokio::sync::mpsc;
use tokio::sync::oneshot;

use crate::{
    messages::RoomStateChangeMessage,
    messages::StateChange,
    messages::{ClientMessages, ServerMessages},
    types::{Story, StoryPoints, Winner},
};

const CHANNEL_SIZE: usize = 100;

/// The state that a Room can be in
#[derive(Debug)]
enum RoomState {
    /// Waiting for a vote
    Idle,
    /// Actually voting
    Voting(CurrentVote),
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
    pub leader: Option<Winner>,
}

#[derive(Debug)]
pub enum SubscriptionResponse {
    Ok(SubscriptionData),
    WinnerExists,
}

#[derive(Debug)]
struct CurrentVote {
    pub story: Story,
    pub votes: Vec<StoryPoints>,
}

impl CurrentVote {
    pub fn new(story: &Story) -> Self {
        Self {
            story: story.clone(),
            votes: Vec::new(),
        }
    }
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
    /// Current leader of the room
    leader: Option<Winner>,
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
            leader: None,
        }
    }

    /// Sets the leader, and notify over channel
    fn set_leader(&mut self, winner: &Winner) {
        // Set new leader, send outgoing messages
        self.leader = Some(winner.clone());
        // Send a message that the leader has changed
        self.send_server_message(ServerMessages::RoomStateChange(
            RoomStateChangeMessage::new(winner.clone(), StateChange::LEADER),
        ));
    }

    /// Subscribe to this room
    fn subscribe(&mut self, subscription_request: &SubscriptionRequest) -> SubscriptionResponse {
        if !self.paricipants.contains(&subscription_request.winner) {
            // Insert participant if it does not exist
            self.paricipants.insert(subscription_request.winner.clone());
            // Get all participants
            let winners = self
                .paricipants
                .iter()
                .map(|winner| winner.clone())
                .collect::<Vec<Winner>>();

            // Set a new leader if it does not exist
            if self.leader.is_none() {
                self.set_leader(&subscription_request.winner);
            }

            // Send the subscription response
            SubscriptionResponse::Ok(SubscriptionData {
                message_receive: self.outgoing.subscribe(),
                winners,
                leader: self.leader.clone(),
            })
        } else {
            SubscriptionResponse::WinnerExists
        }
    }

    /// Unsubscribe from the room
    fn unsubscribe(&mut self, winner: &Winner) {
        self.paricipants.remove(winner);
        // Notify the rest that someone has left
        self.send_server_message(ServerMessages::RoomStateChange(
            RoomStateChangeMessage::new(winner.clone(), StateChange::LEAVE),
        ));
        // Set a new leader if it is available
        if self.paricipants.len() > 0 {
            // Get the first next leader
            let new_leader = self.paricipants.iter().next().unwrap().clone();
            // Set it as the new leader
            self.set_leader(&new_leader)
        }
    }

    /// Send a server messages
    fn send_server_message(&mut self, message: ServerMessages) {
        // Ignore if we are unable to send, it's okay if no one is listening
        let _ = self.outgoing.send(message);
    }

    /// Main loop of the room that is running
    pub async fn run(&mut self) {
        loop {
            match &self.current_state {
                // Waiting for connections and for votes, or leadership changes
                RoomState::Idle => self.idle_loop().await,
                // In voting state
                RoomState::Voting(current_vote) => {}
            }

            // Wait to yield for now
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    }

    /// The loop to run when running in idle mode
    async fn idle_loop(&mut self) {
        while let RoomState::Idle = self.current_state {
            // Check for new subscriptions, skip if there are none
            if let Some(Some(req)) = self.subscription_receiver.recv().now_or_never() {
                println!("Received subscription");
                let response = self.subscribe(&req);
                req.response
                    .send(response)
                    .expect("Could not send subscription response");
                println!("Subscription sent");
            }

            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            // Process incoming messages
            println!("Processing server messages");
            //if let Some(Ok(msg)) = self.incoming.recv().now_or_never() {
            if let Ok(msg) = self.incoming.recv().await {
                println!("Processing server messages 1");
                match msg {
                    ClientMessages::RoomStateChange(change) => match change.change {
                        // Leaving room, so unsubscribe
                        StateChange::LEAVE => self.unsubscribe(&change.winner),
                        _ => {}
                    },
                    ClientMessages::StartVote(msg) => {
                        self.current_state = RoomState::Voting(CurrentVote::new(&msg.story))
                    }
                    // Ignore these messages
                    ClientMessages::Vote(_) => {}
                    ClientMessages::AcknowledgeLeader(_) => {}
                }
            }
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

    /// Subscribe to the Room
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

/// Setup an actual room
pub fn setup_room() -> (Room, RoomCommunication) {
    // Create channels for subscriptions
    let (subscription_sender, subscription_receiver) = mpsc::channel(CHANNEL_SIZE);

    // Create channel for server messages
    let (server_sender, _) = channel(CHANNEL_SIZE);

    // Create channel for client messages
    let (client_sender, client_receiver) = channel(CHANNEL_SIZE);
    (
        Room::new(subscription_receiver, client_receiver, server_sender),
        RoomCommunication::new(client_sender, subscription_sender),
    )
}

#[cfg(test)]
mod tests {
    use crate::{
        messages::ClientMessages::RoomStateChange, messages::RoomStateChangeMessage,
        messages::ServerMessages, messages::StateChange, types::Winner,
    };

    use super::{setup_room, SubscriptionData, SubscriptionResponse};

    /// Test whether the subscription response is ok
    fn subscription_response_ok(response: SubscriptionResponse) -> SubscriptionData {
        match response {
            SubscriptionResponse::Ok(data) => data,
            SubscriptionResponse::WinnerExists => panic!("Winner already exists"),
        }
    }

    #[tokio::test]
    pub async fn test_subscription() {
        let winner = Winner("Me".into());
        let rival = Winner("Rival".into());

        let (mut room, communcation) = setup_room();

        // Run the room
        tokio::spawn(async move { room.run().await });

        // Try to create a subscription
        let _ = subscription_response_ok(communcation.subscribe(&winner).await);

        // Try to create another, subscription
        let data = subscription_response_ok(communcation.subscribe(&rival).await);
        assert!(data.winners.contains(&rival));
        assert_eq!(data.leader.expect("Expected leader to be set"), winner);
    }

    #[tokio::test]
    pub async fn test_leaving() {
        let winner = Winner("Me".into());
        let rival = Winner("Rival".into());

        let (mut room, communication) = setup_room();

        // Run the room
        tokio::spawn(async move { room.run().await });

        // Try to create a subscription
        let _ = subscription_response_ok(communication.subscribe(&winner).await);

        // Try to create another, subscription
        let mut data = subscription_response_ok(communication.subscribe(&rival).await);

        // I leave the room
        communication
            .client_sender
            .send(RoomStateChange(RoomStateChangeMessage::new(
                winner.clone(),
                StateChange::LEAVE,
            )))
            .unwrap();

        // I receive a room leave message
        let msg = data
            .message_receive
            .recv()
            .await
            .expect("Could not receive message");
        if let ServerMessages::RoomStateChange(change) = msg {
            assert_eq!(change.change, StateChange::LEAVE);
            assert_eq!(change.winner, winner);
        }

        // I receive a leadership change message
        let msg = data
            .message_receive
            .recv()
            .await
            .expect("Could not receive message");
        if let ServerMessages::RoomStateChange(change) = msg {
            assert_eq!(change.change, StateChange::LEADER);
            assert_eq!(change.winner, rival);
        }
    }
}
