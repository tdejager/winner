use crate::messages::{ClientMessages, ServerMessages, StateChange};
use crate::room::{
    ClientMessageRequest, Room, SubscriptionRequest, SubscriptionResponse, CHANNEL_SIZE,
};
use crate::types::{Story, StoryPoints, Winner};
use tokio::sync::{broadcast, mpsc, oneshot};

/// Contains all the channels to communicate with a Room
#[derive(Clone)]
pub struct RoomSubscriber {
    /// Sender to which can be cloned to create server messages
    pub client_sender: mpsc::Sender<ClientMessageRequest>,
    /// Sender to try to get a subscription to the room
    pub subscription_sender: mpsc::Sender<SubscriptionRequest>,
}

pub struct RoomInitialState {
    pub winners: Vec<Winner>,
    pub leader: Option<Winner>,
}

impl RoomSubscriber {
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
    ) -> anyhow::Result<(RoomAPI, RoomStateUpdater, RoomInitialState)> {
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
                RoomAPI::new(winner.clone(), self.client_sender.clone()),
                RoomStateUpdater::new(data.message_receive),
                RoomInitialState {
                    winners: data.winners,
                    leader: data.leader,
                },
            )),
            SubscriptionResponse::WinnerExists => anyhow::bail!("Winner already exists"),
        }
    }
}

/// Setup an actual room
/// returns the Room struct that should be spawned separately and an subscriber that can be used to get a unique subscription to the room
pub fn setup_room() -> (Room, RoomSubscriber) {
    // Create channels for subscriptions
    let (subscription_sender, subscription_receiver) = mpsc::channel(CHANNEL_SIZE);

    // Create channel for server messages
    let (server_sender, _) = broadcast::channel(CHANNEL_SIZE);

    // Create channel for client messages
    let (client_sender, client_receiver) = mpsc::channel(CHANNEL_SIZE);
    (
        Room::new(subscription_receiver, client_receiver, server_sender),
        RoomSubscriber::new(client_sender, subscription_sender),
    )
}

/// Struct to communicate with the room from the client side, presenting a nice user API
pub struct RoomAPI {
    pub winner: Winner,
    pub message_send: mpsc::Sender<ClientMessageRequest>,
}

impl RoomAPI {
    pub fn new(winner: Winner, message_send: mpsc::Sender<ClientMessageRequest>) -> Self {
        Self {
            winner,
            message_send,
        }
    }

    /// Send a client message to the sever
    async fn send_client_request(&self, message: ClientMessages) -> anyhow::Result<()> {
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
    async fn fire_and_forget(&self, message: ClientMessages) -> anyhow::Result<()> {
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
    pub async fn start_vote(&self, story: &Story) -> anyhow::Result<()> {
        self.send_client_request(ClientMessages::StartVote(story.clone()))
            .await?;
        Ok(())
    }

    /// Cast a vote
    pub async fn cast_vote(&self, story: &Story, points: StoryPoints) -> anyhow::Result<()> {
        self.send_client_request(ClientMessages::Vote((
            self.winner.clone(),
            story.clone(),
            points.clone(),
        )))
        .await?;
        Ok(())
    }
}

/// This struct is used to receive state
pub struct RoomStateUpdater {
    pub message_receive: broadcast::Receiver<ServerMessages>,
}

impl RoomStateUpdater {
    pub fn new(message_receive: broadcast::Receiver<ServerMessages>) -> Self {
        Self { message_receive }
    }

    /// Receives a server message
    pub async fn receive_message(&mut self) -> anyhow::Result<ServerMessages> {
        Ok(self.message_receive.recv().await?)
    }

    /// Skip messages
    pub async fn skip_messages(&mut self, count: usize) -> anyhow::Result<()> {
        for _ in 0..count {
            self.message_receive.recv().await?;
        }
        Ok(())
    }

    /// Skip messages but also debug print each message
    pub async fn skip_and_debug_messages(&mut self, count: usize) -> anyhow::Result<()> {
        for _ in 0..count {
            dbg!(self.message_receive.recv().await?);
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

    use super::{setup_room, RoomAPI, RoomSubscriber};
    use crate::room_communication::RoomStateUpdater;

    /// Contains test info
    struct TestSetup {
        winner_api: RoomAPI,
        winner_state_update: RoomStateUpdater,
        rival_api: RoomAPI,
        rival_state_update: RoomStateUpdater,
        _room_communication: RoomSubscriber,
        _room_handle: JoinHandle<()>,
    }

    /// Scaffolding test code
    async fn setup_test() -> TestSetup {
        let winner = Winner("Me".into());
        let rival = Winner("Rival".into());

        let (mut room, room_communication) = setup_room();

        // Run the room
        let _room_handle = tokio::spawn(async move { room.run().await });

        // Try to create a subscription
        let (winner_api, mut winner_state_update, ..) = room_communication
            .subscribe(&winner)
            .await
            .expect("Could not subscribe winner");

        // Try to create another, subscription
        let (rival_api, mut rival_state_update, ..) = room_communication
            .subscribe(&rival)
            .await
            .expect("Could not subscribe rival");

        // Skip rival and winner entering room
        winner_state_update.skip_messages(1).await.unwrap();
        rival_state_update.skip_messages(1).await.unwrap();

        TestSetup {
            winner_api,
            winner_state_update,
            rival_api,
            rival_state_update,
            _room_communication: room_communication,
            _room_handle,
        }
    }

    /// Await message and check if it is the correct type
    macro_rules! await_and_msg {
        ($a:expr, $p:pat) => {
            assert!(matches!($a.await.unwrap(), $p))
        };
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
        let (_, mut state_update, initial_state) = communication.subscribe(&rival).await.unwrap();

        // Check initial state
        assert!(initial_state.winners.contains(&rival));

        // We expect a leader to be set to the first person in the room
        assert_eq!(
            initial_state.leader.expect("Expected leader to be set"),
            winner
        );

        // I get a message that I have entered
        await_and_msg!(
            state_update.receive_message(),
            ServerMessages::RoomParticipantsChange(_)
        );
    }

    #[tokio::test]
    pub async fn test_leaving() {
        let TestSetup {
            winner_api,
            rival_api,
            mut rival_state_update,
            ..
        } = setup_test().await;

        // I leave the room
        winner_api.leave_room().await.expect("Could not leave room");

        // I receive a room leave message
        let msg = rival_state_update.receive_message().await.unwrap();

        if let ServerMessages::RoomParticipantsChange(change) = msg {
            let (winner, state_change) = change;
            assert_eq!(state_change, StateChange::Leave);
            assert_eq!(winner, winner);
        } else {
            panic!("Wrong message type received {:?}", msg);
        }

        // I receive a leadership change message
        let msg = rival_state_update
            .message_receive
            .recv()
            .await
            .expect("Could not receive message");

        if let ServerMessages::RoomParticipantsChange(change) = msg {
            let (winner, state_change) = change;
            assert_eq!(state_change, StateChange::Leader);
            assert_eq!(winner, rival_api.winner);
        } else {
            panic!("Wrong message type received {:?}", msg);
        }
    }

    #[tokio::test]
    pub async fn test_voting_single() {
        let TestSetup {
            winner_api,
            mut winner_state_update,
            rival_api,
            ..
        } = setup_test().await;

        // Rival leaves room
        rival_api.leave_room().await.unwrap();

        // Skip 2 messages, containing enter, leaving and leadership change
        winner_state_update.skip_messages(3).await.unwrap();

        let story = Story::new(StoryId(0), "New story");

        winner_api.start_vote(&story).await.unwrap();

        // We should receive a message that the room state has changed
        await_and_msg!(
            winner_state_update.receive_message(),
            ServerMessages::RoomStateChange(RoomStateChange::Voting)
        );

        // We should receive a message that a vote has been started
        await_and_msg!(
            winner_state_update.receive_message(),
            ServerMessages::StartVote(_)
        );

        // Cast a vote
        winner_api
            .cast_vote(&story, StoryPoints::ONE)
            .await
            .unwrap();

        // We should receive a message that our vote has been cast
        await_and_msg!(
            winner_state_update.receive_message(),
            ServerMessages::VoteCast(_)
        );

        // We should receive a message that the room state has changed
        await_and_msg!(
            winner_state_update.receive_message(),
            ServerMessages::VotesReceived(_)
        );
        // And room should go back to idle
        // We should receive a message that the room state has changed
        await_and_msg!(
            winner_state_update.receive_message(),
            ServerMessages::RoomStateChange(RoomStateChange::Idle)
        );
    }

    // TODO: Create test for multiple votes + fight
    // TODO: Create test if multiple voters have the same vote that there is no fight
}
