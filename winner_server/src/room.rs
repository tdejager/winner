#![allow(dead_code)]

use std::collections::{HashMap, HashSet};

use anyhow::bail;
use anyhow::Result;
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

/// Struct containing a client message and a channel to send a result
#[derive(Debug)]
pub struct ClientMessageRequest {
    message: ClientMessages,
    response_channel: oneshot::Sender<anyhow::Result<()>>,
}

/// Implementation of a room
pub struct Room {
    current_state: RoomState,
    /// Current paricipants in the room
    participants: HashSet<Winner>,
    /// Incoming message that need to be processed
    incoming: mpsc::Receiver<ClientMessageRequest>,
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
        incoming: mpsc::Receiver<ClientMessageRequest>,
        outgoing: Sender<ServerMessages>,
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

/// Contains all the channels to communicate with a Room
#[derive(Clone)]
pub struct RoomCommunication {
    /// Sender to which can be cloned to create server messages
    pub client_sender: mpsc::Sender<ClientMessageRequest>,
    /// Sender to try to get a subscription to the room
    pub subscription_sender: mpsc::Sender<SubscriptionRequest>,
}

impl RoomCommunication {
    pub fn new(
        client_sender: mpsc::Sender<ClientMessageRequest>,
        subscription_sender: mpsc::Sender<SubscriptionRequest>,
    ) -> Self {
        Self {
            client_sender,
            subscription_sender,
        }
    }

    /// Subscribe to the Room
    /// returns a struct to communicate with the room and the initial state a Vector of winners and an optional winner
    pub async fn subscribe(
        &self,
        winner: &Winner,
    ) -> anyhow::Result<(WinnerRoomCommunication, Vec<Winner>, Option<Winner>)> {
        let (tx, rx) = oneshot::channel();
        self.subscription_sender
            .send(SubscriptionRequest {
                winner: winner.clone(),
                response: tx,
            })
            .await
            .expect("Could not send subscription request");

        // Wait for the response and construct required objects
        match rx.await? {
            SubscriptionResponse::Ok(data) => Ok((
                WinnerRoomCommunication::new(
                    winner.clone(),
                    self.client_sender.clone(),
                    data.message_receive,
                ),
                data.winners,
                data.leader,
            )),
            SubscriptionResponse::WinnerExists => bail!("Winner already exists"),
        }
    }
}

/// Setup an actual room
pub fn setup_room() -> (Room, RoomCommunication) {
    // Create channels for subscriptions
    let (subscription_sender, subscription_receiver) = mpsc::channel(CHANNEL_SIZE);

    // Create channel for server messages
    let (server_sender, _) = channel(CHANNEL_SIZE);

    // Create channel for client messages
    let (client_sender, client_receiver) = mpsc::channel(CHANNEL_SIZE);
    (
        Room::new(subscription_receiver, client_receiver, server_sender),
        RoomCommunication::new(client_sender, subscription_sender),
    )
}

/// Struct to communicate with the room from the client side
pub struct WinnerRoomCommunication {
    pub winner: Winner,
    pub message_send: mpsc::Sender<ClientMessageRequest>,
    pub message_receive: Receiver<ServerMessages>,
}

impl WinnerRoomCommunication {
    pub fn new(
        winner: Winner,
        message_send: mpsc::Sender<ClientMessageRequest>,
        message_receive: Receiver<ServerMessages>,
    ) -> Self {
        Self {
            winner,
            message_send,
            message_receive,
        }
    }

    /// Send a client message to the sever
    async fn send_client_request(&self, message: ClientMessages) -> Result<()> {
        let (tx, rx) = oneshot::channel();
        self.message_send
            .send(ClientMessageRequest {
                message,
                response_channel: tx,
            })
            .await?;
        // Await the response
        let response = rx.await?;
        // Propagate the error if the response failed, otherwise return ok
        Ok(response?)
    }

    /// Send a message to the room and ignore the result
    async fn fire_and_forget(&self, message: ClientMessages) -> Result<()> {
        let (tx, _) = oneshot::channel();
        self.message_send
            .send(ClientMessageRequest {
                message,
                response_channel: tx,
            })
            .await?;
        Ok(())
    }

    /// Leave the room, consumes this object
    pub async fn leave_room(self) -> anyhow::Result<()> {
        self.fire_and_forget(ClientMessages::RoomStateChange((
            self.winner.clone(),
            StateChange::Leave,
        )))
        .await?;
        Ok(())
    }

    /// Start a vote
    pub async fn start_vote(&self, story: &Story) -> Result<()> {
        self.send_client_request(ClientMessages::StartVote(story.clone()))
            .await?;
        Ok(())
    }

    /// Cast a vote
    pub async fn cast_vote(&self, story: &Story, points: StoryPoints) -> Result<()> {
        self.send_client_request(ClientMessages::Vote((
            self.winner.clone(),
            story.clone(),
            points.clone(),
        )))
        .await?;
        Ok(())
    }

    /// Receive a server message
    pub async fn receive_message(&mut self) -> ServerMessages {
        self.message_receive
            .recv()
            .await
            .expect("Could not receive server message")
    }

    /// Skip messages
    pub async fn skip_messages(&mut self, count: usize) -> Result<()> {
        for _ in 0..count {
            self.message_receive.recv().await?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use tokio::task::JoinHandle;

    use crate::{
        messages::RoomStateChange,
        messages::ServerMessages,
        messages::StateChange,
        types::{Story, StoryId, StoryPoints, Winner},
    };

    use super::{setup_room, RoomCommunication, WinnerRoomCommunication};

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
        let (winner_communication, ..) = room_communication
            .subscribe(&winner)
            .await
            .expect("Could not subscribe winner");

        // Try to create another, subscription
        let (rival_communication, ..) = room_communication
            .subscribe(&rival)
            .await
            .expect("Could not subscribe rival");

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
        let _ = communication.subscribe(&winner).await.unwrap();

        // Try to create another, subscription
        let (_, winners, leader) = communication.subscribe(&rival).await.unwrap();

        assert!(winners.contains(&rival));
        assert_eq!(leader.expect("Expected leader to be set"), winner);
    }

    #[tokio::test]
    pub async fn test_leaving() {
        let TestSetup {
            winner_communication,
            mut rival_communication,
            ..
        } = setup_test().await;

        // I leave the room
        winner_communication
            .leave_room()
            .await
            .expect("Could not leave room");

        // I receive a room leave message
        let msg = rival_communication.receive_message().await;

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

    macro_rules! await_and_msg {
        ($a:expr, $p:pat) => {
            assert!(matches!($a.await, $p))
        };
    }

    #[tokio::test]
    pub async fn test_voting_single() {
        let TestSetup {
            mut winner_communication,
            rival_communication,
            ..
        } = setup_test().await;

        // Rival leaves room
        rival_communication.leave_room().await.unwrap();

        // Skip 2 messages, containing leaving and leadership change
        winner_communication.skip_messages(2).await.unwrap();

        let story = Story::new(StoryId(0), "New story");

        winner_communication.start_vote(&story).await.unwrap();

        // We should receive a message that the room state has changed
        await_and_msg!(
            winner_communication.receive_message(),
            ServerMessages::RoomStateChange(RoomStateChange::Voting)
        );

        // We should receive a message that a vote has been started
        await_and_msg!(
            winner_communication.receive_message(),
            ServerMessages::StartVote(_)
        );

        // Cast a vote
        winner_communication
            .cast_vote(&story, StoryPoints::ONE)
            .await
            .unwrap();

        // We should receive a message that our vote has been cast
        await_and_msg!(
            winner_communication.receive_message(),
            ServerMessages::VoteCast(_)
        );

        // We should receive a message that the room state has changed
        await_and_msg!(
            winner_communication.receive_message(),
            ServerMessages::VotesReceived(_)
        );
        // And room should go back to idle
        // We should receive a message that the room state has changed
        await_and_msg!(
            winner_communication.receive_message(),
            ServerMessages::RoomStateChange(RoomStateChange::Idle)
        );
    }

    // TODO: Create test for multiple votes + fight
    // TODO: Create test if multiple voters have the same vote that there is no fight
}
