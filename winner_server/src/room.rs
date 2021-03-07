use std::collections::{HashMap, HashSet};

use tokio::sync::broadcast;
use tokio::sync::mpsc;
use tokio::sync::oneshot;

use crate::{
    messages::StateChange,
    messages::{ClientMessages, RoomStateChange, ServerMessages},
    types::{Story, StoryPoints, Winner},
};

pub const CHANNEL_SIZE: usize = 100;

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
    pub message_receive: broadcast::Receiver<ServerMessages>,
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

/// Struct containing a client message and a channel to send a result
#[derive(Debug)]
pub struct ClientMessageRequest {
    pub message: ClientMessages,
    pub response_channel: oneshot::Sender<anyhow::Result<()>>,
}

/// Implementation of a room
pub struct Room {
    current_state: RoomState,
    /// Current paricipants in the room
    participants: HashSet<Winner>,
    /// Incoming message that need to be processed
    incoming: mpsc::Receiver<ClientMessageRequest>,
    /// Outgoing message that will be processed by the clients
    outgoing: broadcast::Sender<ServerMessages>,
    /// Receives subscription request
    subscription_receiver: mpsc::Receiver<SubscriptionRequest>,
    /// Current leader of the room
    leader: Option<Winner>,
}

impl Room {
    /// Create a new room
    pub fn new(
        subscription_receiver: mpsc::Receiver<SubscriptionRequest>,
        incoming: mpsc::Receiver<ClientMessageRequest>,
        outgoing: broadcast::Sender<ServerMessages>,
    ) -> Self {
        Room {
            participants: HashSet::new(),
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
        if !self.participants.contains(&subscription_request.winner) {
            // Insert participant if it does not exist
            self.participants
                .insert(subscription_request.winner.clone());
            // Get all participants
            let winners = self
                .participants
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
        self.participants.remove(winner);
        // Notify the rest that someone has left
        self.send_server_message(ServerMessages::RoomParticipantsChange((
            winner.clone(),
            StateChange::Leave,
        )));
        // Set a new leader if it is available
        if self.participants.len() > 0 {
            // Get the first next leader
            let new_leader = self.participants.iter().next().unwrap().clone();
            // Set it as the new leader
            self.set_leader(&new_leader)
        }
    }

    /// Receive message from the incoming channel
    async fn receive_incoming_messages(&mut self) -> ClientMessageRequest {
        self.incoming
            .recv()
            .await
            .expect("Could not receive incoming messages")
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
                Some(msg) = self.incoming.recv() => {
                    println!("Processing server messages");
                    let response_channel = msg.response_channel;
                    match msg.message {
                        ClientMessages::RoomStateChange((winner, change)) => match change {
                            // Leaving room, so unsubscribe
                            StateChange::Leave => self.unsubscribe(&winner),
                            _ => {}
                        },
                        // A new vote is requested
                        ClientMessages::StartVote(story) => {
                            response_channel.send(Ok(())).expect("Could not send 'StartVote' result");
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

    /// Handles the voting loop
    async fn voting(&mut self, mut vote: CurrentVote) {
        println!("Starting vote on story {}", vote.story.title);
        // Notify clients that a vote has started
        self.send_server_message(ServerMessages::StartVote(vote.story.clone()));

        println!("Waiting for votes");
        // Wait for votes to come in
        while self.participants.len() > vote.votes.len() {
            let ClientMessageRequest {
                message,
                response_channel,
            } = self.receive_incoming_messages().await;

            match message {
                // Process actual vote
                ClientMessages::Vote((winner, story, story_points)) => {
                    println!("Vote received for {} by {:?}", story.title, winner);
                    if story == vote.story {
                        vote.votes.insert(winner.clone(), story_points.clone());
                        println!(
                            "Received {}/{} votes",
                            vote.votes.len(),
                            self.participants.len()
                        );
                    }
                    // Communicate that a vote has been cast
                    response_channel
                        .send(Ok(()))
                        .expect("Could not send response");
                    self.send_server_message(ServerMessages::VoteCast((winner, story_points)));
                }
                ClientMessages::RoomStateChange((winner, change)) => match change {
                    // Leaving room, so unsubscribe, we do not need to include participant for the count
                    StateChange::Leave => self.unsubscribe(&winner),
                    _ => {}
                },

                // Other messages cannot be processed in this state
                _ => response_channel
                    .send(Err(anyhow::anyhow!(
                        "Cant process this message when in 'Voting' state"
                    )))
                    .expect("Could not sends response"),
            }
        }

        println!("Votes received");
        // Broadcast results, let everyone know
        self.send_server_message(ServerMessages::VotesReceived(vote.votes.clone()));

        // Start a fight between participants if the votes are unequal
        // This check checks if the min and the max of the vector are the same
        // if they are they only contain the same votes
        // skip this if there is only a single participant as well
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
                    && self.participants.contains(highest_winner)
                    && self.participants.contains(lowest_winner)
                {
                    let ClientMessageRequest {
                        message,
                        response_channel,
                    } = self.receive_incoming_messages().await;
                    match message {
                        ClientMessages::FightResolved => fight_resolved = true,
                        ClientMessages::RoomStateChange((winner, change)) => match change {
                            // Leaving room, so unsubscribe, we do not need to include participant for the count
                            StateChange::Leave => self.unsubscribe(&winner),
                            _ => response_channel
                                .send(Err(anyhow::anyhow!("Can't process this RoomStateChange")))
                                .expect("Could not sends message"),
                        },
                        _ => response_channel
                            .send(Err(anyhow::anyhow!(
                                "Can't process this message when in 'Fighting' state"
                            )))
                            .expect("Could not sends response"),
                    }
                }
            }
        }

        // Back to Idle state
        self.set_room_state(RoomState::Idle);
    }
}
