#![allow(dead_code)]

use std::collections::{HashMap, HashSet};

use tokio::sync::broadcast::{channel, Receiver, Sender};
use tokio::sync::mpsc;
use tokio::sync::oneshot;

use crate::{
    messages::StateChange,
    messages::{ClientMessages, RoomStateChange, ServerMessages},
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

#[derive(Debug, Clone)]
struct CurrentVote {
    pub story: Story,
    pub votes: HashMap<Winner, StoryPoints>,
}

impl CurrentVote {
    pub fn new(story: &Story) -> Self {
        Self {
            story: story.clone(),
            votes: HashMap::new(),
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
        self.send_server_message(ServerMessages::RoomParticipantsChange((
            winner.clone(),
            StateChange::Leader,
        )));
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
        self.send_server_message(ServerMessages::RoomParticipantsChange((
            winner.clone(),
            StateChange::Leave,
        )));
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

    /// Set a new room state
    fn set_room_state(&mut self, room_state: RoomState) {
        // Update the current state
        self.current_state = room_state;
        let change = match self.current_state {
            RoomState::Idle => ServerMessages::RoomStateChange(RoomStateChange::Idle),
            RoomState::Voting(_) => ServerMessages::RoomStateChange(RoomStateChange::Voting),
        };
        // Send a change message
        self.send_server_message(change);
    }
}

impl Room {
    /// Main loop of the room that is running
    pub async fn run(&mut self) {
        loop {
            match &self.current_state {
                // Waiting for connections and for votes, or leadership changes
                RoomState::Idle => self.idle().await,
                // In voting state
                RoomState::Voting(current_vote) => {
                    let current_vote = current_vote.clone();
                    self.voting(current_vote).await;
                }
            }
        }
    }

    /// The loop to run when running in idle mode
    async fn idle(&mut self) {
        while let RoomState::Idle = self.current_state {
            tokio::select! {

                // Subscriber future
                Some(req) = self.subscription_receiver.recv() => {
                    println!("Received subscription");
                    let response = self.subscribe(&req);
                    req.response
                        .send(response)
                        .expect("Could not send subscription response");
                    println!("Subscription sent");
                },

                // Incoming messages future
                Ok(msg) = self.incoming.recv() => {
                    println!("Processing server messages");
                    match msg {
                        // TODO: maybe move this to a seperate task, so we do not need to repeat
                        // this everywhere?
                        ClientMessages::RoomStateChange((winner, change)) => match change {
                            // Leaving room, so unsubscribe
                            StateChange::Leave => self.unsubscribe(&winner),
                            _ => {}
                        },
                        // A new vote is requested
                        ClientMessages::StartVote(story) => {
                            self.send_server_message(ServerMessages::RoomStateChange(RoomStateChange::Voting));
                            self.current_state = RoomState::Voting(CurrentVote::new(&story));
                        }
                        // Ignore these messages for now
                        _ => {},
                    }
                }
            }
        }
    }

    async fn voting(&mut self, mut vote: CurrentVote) {
        // Notify clients that a vote has started
        self.send_server_message(ServerMessages::StartVote(vote.story.clone()));

        // Wait for votes to come in
        while self.paricipants.len() != vote.votes.len() {
            match self
                .incoming
                .recv()
                .await
                .expect("No more incoming messages could be received")
            {
                // Process actual vote
                ClientMessages::Vote((winner, story, story_points)) => {
                    if story == vote.story {
                        vote.votes.insert(winner, story_points);
                    }
                }
                ClientMessages::RoomStateChange((winner, change)) => match change {
                    // Leaving room, so unsubscribe, we do not need to include participant for the count
                    StateChange::Leave => self.unsubscribe(&winner),
                    _ => {}
                },
                _ => {}
            }
        }

        // Broadcast results, let everyone know
        self.send_server_message(ServerMessages::VotesReceived(vote.votes.clone()));

        // Start a fight between participants if the votes are unequal
        // This check checks if the min and the max of the vector are the same
        // if they are they only contain the same votes
        // skip this if there is only a single particpant as well
        if vote.votes.len() > 1
            && vote.votes.iter().map(|a| a.1).max() != vote.votes.iter().map(|a| a.1).min()
        {
            // Choose highest and lowest winners to do a fight
            let highest_entry = vote.votes.iter().max_by(|x, y| x.1.cmp(y.1));
            let lowest_entry = vote.votes.iter().min_by(|x, y| x.1.cmp(y.1));

            if highest_entry.is_some() && lowest_entry.is_some() {
                let highest_winner = highest_entry.unwrap().0;
                let lowest_winner = lowest_entry.unwrap().0;
                // Send out a fight message
                self.send_server_message(ServerMessages::Fight((
                    highest_entry.unwrap().0.clone(),
                    lowest_entry.unwrap().0.clone(),
                )));

                // Wait for the fight to be resolved
                let mut fight_resolved = false;
                while !fight_resolved
                    && self.paricipants.contains(highest_winner)
                    && self.paricipants.contains(lowest_winner)
                {
                    match self
                        .incoming
                        .recv()
                        .await
                        .expect("No more incoming messages could be received")
                    {
                        ClientMessages::FightResolved => fight_resolved = true,
                        ClientMessages::RoomStateChange((winner, change)) => match change {
                            // Leaving room, so unsubscribe, we do not need to include participant for the count
                            StateChange::Leave => self.unsubscribe(&winner),
                            _ => {}
                        },
                        _ => {}
                    }
                }
            }
        }

        // Back to Idle state
        self.set_room_state(RoomState::Idle);
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

/// Struct to communicate with the room from the client side
pub struct WinnerRoomCommunication {
    pub winner: Winner,
    pub message_send: Sender<ClientMessages>,
    pub message_receive: Receiver<ServerMessages>,
}

impl WinnerRoomCommunication {
    pub fn new(
        winner: Winner,
        message_send: Sender<ClientMessages>,
        message_receive: Receiver<ServerMessages>,
    ) -> Self {
        Self {
            winner,
            message_send,
            message_receive,
        }
    }

    /// Send a client message to the sever
    fn send_client_message(&self, message: ClientMessages) {
        self.message_send
            .send(message)
            .expect("Unable to send client message");
    }

    /// Leave the room, consumes this object
    pub fn leave_room(self) {
        self.message_send
            .send(ClientMessages::RoomStateChange((
                self.winner.clone(),
                StateChange::Leave,
            )))
            .expect("Unable to send room state change");
    }

    /// Start a vote
    pub fn start_vote(&self, story: &Story) {
        self.send_client_message(ClientMessages::StartVote(story.clone()));
    }

    /// Cast a vote
    pub fn cast_vote(&self, story: &Story, points: StoryPoints) {
        self.send_client_message(ClientMessages::Vote((
            self.winner.clone(),
            story.clone(),
            points.clone(),
        )));
    }
}

#[cfg(test)]
mod tests {
    use tokio::task::JoinHandle;

    use crate::{
        messages::RoomStateChange,
        messages::ServerMessages,
        messages::StateChange,
        types::{Story, StoryId, Winner},
    };

    use super::{
        setup_room, RoomCommunication, SubscriptionData, SubscriptionResponse,
        WinnerRoomCommunication,
    };

    /// Test whether the subscription response is ok
    fn subscription_response_ok(response: SubscriptionResponse) -> SubscriptionData {
        println!("subscription_response_ok");
        match response {
            SubscriptionResponse::Ok(data) => data,
            SubscriptionResponse::WinnerExists => panic!("Winner already exists"),
        }
    }

    /// Contains test info
    struct TestSetup {
        winner_communication: WinnerRoomCommunication,
        rival_communication: WinnerRoomCommunication,
        room_communication: RoomCommunication,
        room_handle: JoinHandle<()>,
    }

    /// Scaffolding test code
    async fn setup_test() -> TestSetup {
        let winner = Winner("Me".into());
        let rival = Winner("Rival".into());

        let (mut room, room_communication) = setup_room();

        // Run the room
        let room_handle = tokio::spawn(async move { room.run().await });

        // Try to create a subscription
        let winner_data = subscription_response_ok(room_communication.subscribe(&winner).await);

        // Try to create another, subscription
        let rival_data = subscription_response_ok(room_communication.subscribe(&rival).await);

        // Winner communication
        let winner_communication = WinnerRoomCommunication::new(
            winner.clone(),
            room_communication.client_sender.clone(),
            winner_data.message_receive,
        );

        // Rival communication
        let rival_communication = WinnerRoomCommunication::new(
            rival.clone(),
            room_communication.client_sender.clone(),
            rival_data.message_receive,
        );

        TestSetup {
            winner_communication,
            rival_communication,
            room_communication,
            room_handle,
        }
    }

    #[tokio::test]
    pub async fn test_subscription() {
        let winner = Winner("Me".into());
        let rival = Winner("Rival".into());

        let (mut room, communication) = setup_room();

        // Run the room
        tokio::spawn(async move { room.run().await });

        // Try to create a subscription
        let _ = subscription_response_ok(communication.subscribe(&winner).await);

        // Try to create another, subscription
        let data = subscription_response_ok(communication.subscribe(&rival).await);

        assert!(data.winners.contains(&rival));
        assert_eq!(data.leader.expect("Expected leader to be set"), winner);
    }

    #[tokio::test]
    pub async fn test_leaving() {
        let TestSetup {
            winner_communication,
            mut rival_communication,
            ..
        } = setup_test().await;

        // Clone the winner so we can compare it later
        let winner = winner_communication.winner.clone();
        // I leave the room
        winner_communication.leave_room();

        // I receive a room leave message
        let msg = rival_communication
            .message_receive
            .recv()
            .await
            .expect("Could not receive message");

        if let ServerMessages::RoomParticipantsChange(change) = msg {
            let (winner, state_change) = change;
            assert_eq!(state_change, StateChange::Leave);
            assert_eq!(winner, winner);
        } else {
            panic!("Wrong message type received {:?}", msg);
        }

        // I receive a leadership change message
        let msg = rival_communication
            .message_receive
            .recv()
            .await
            .expect("Could not receive message");

        if let ServerMessages::RoomParticipantsChange(change) = msg {
            let (winner, state_change) = change;
            assert_eq!(state_change, StateChange::Leader);
            assert_eq!(winner, rival_communication.winner);
        } else {
            panic!("Wrong message type received {:?}", msg);
        }
    }

    #[tokio::test]
    pub async fn test_voting() {
        let TestSetup {
            mut winner_communication,
            ..
        } = setup_test().await;

        let story = Story::new(StoryId(0), "New story");

        winner_communication.start_vote(&story);

        let msg = winner_communication
            .message_receive
            .recv()
            .await
            .expect("Could not receive state change message");

        // We should receive a message that the room state has changed
        assert!(matches!(
            msg,
            ServerMessages::RoomStateChange(RoomStateChange::Voting)
        ));

        let msg = winner_communication
            .message_receive
            .recv()
            .await
            .expect("Could not receive state change message");

        // We should receive a message that the room state has changed
        assert!(matches!(msg, ServerMessages::StartVote(_)));
    }
}
