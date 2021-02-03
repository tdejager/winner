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

struct SubscriptionRequest {
    pub winner: Winner,
    pub response: oneshot::Sender<SubscriptionResponse>,
}

struct Subscription {}

#[derive(Debug)]
enum SubscriptionResponse {
    Ok(Receiver<ServerMessages>),
    WinnerExists,
}

/// Implementation of a room
struct Room {
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
    fn subscribe(&self, subscription_request: &SubscriptionRequest) -> SubscriptionResponse {
        if !self.paricipants.contains(&subscription_request.winner) {
            SubscriptionResponse::Ok(self.outgoing.subscribe())
        } else {
            SubscriptionResponse::WinnerExists
        }
    }

    /// Main loop of the r
    async fn run(&mut self) {
        // Check for new subscriptions
        // subscribe if possible
        if let Some(Some(req)) = self.subscription_receiver.recv().now_or_never() {
            let response = self.subscribe(&req);
            req.response
                .send(response)
                .expect("Could not send subscription response");
        }
    }
}

/// Contains all the channels to communicate with a Room
struct RoomCommunication {
    /// Sender to which can be cloned to create server messages
    sender: Sender<ServerMessages>,
    /// Sender to try to get a subscription to the room
    subscription_sender: mpsc::Sender<SubscriptionRequest>,
}
