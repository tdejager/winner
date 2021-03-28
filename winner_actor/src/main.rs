mod messages;

use crate::messages::server::RoomStateChanged;
use actix::prelude::*;
use rand::seq::IteratorRandom;
use std::collections::{HashMap, HashSet};
use std::ops::Deref;
use winner_server::messages::{RoomInitialState, StateChange};
use winner_server::types::{Story, StoryId, StoryPoints, Vote, Winner};

#[derive(Clone, Debug, Default)]
pub struct RoomState {
    pub winners: Vec<Winner>,
    pub leader: Option<Winner>,
    pub voting_state: RoomVotingState,
    pub past_votes: HashMap<Story, StoryPoints>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ActiveVote {
    pub participants: HashMap<Winner, Option<StoryPoints>>,
    pub story: Story,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Fight {
    pub fighters: Vec<(Winner, StoryPoints)>,
    pub participants: HashMap<Winner, StoryPoints>,
    pub story: Story,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RoomVotingState {
    Idle,
    Voting(ActiveVote),
    Fighting(Fight),
}

impl Default for RoomVotingState {
    fn default() -> Self {
        RoomVotingState::Idle
    }
}

struct Room {
    /// Name of the room
    pub name: String,
    /// Leader that can start the votes
    pub leader: Option<Winner>,
    /// The active voting state
    pub voting_state: RoomVotingState,
    /// Past votes
    pub past_votes: HashMap<Story, StoryPoints>,
    /// People participating in the room
    winners: HashMap<Winner, Recipient<messages::server::RoomStateChanged>>,
}

impl Into<RoomState> for &Room {
    fn into(self) -> RoomState {
        RoomState {
            winners: self.winners.keys().cloned().collect(),
            leader: self.leader.clone(),
            voting_state: self.voting_state.clone(),
            past_votes: self.past_votes.clone(),
        }
    }
}

impl Actor for Room {
    type Context = Context<Self>;
}

impl Handler<messages::client::Enter> for Room {
    type Result = anyhow::Result<RoomState>;

    fn handle(&mut self, msg: messages::client::Enter, ctx: &mut Self::Context) -> Self::Result {
        if self.winners.contains_key(&msg.winner) {
            anyhow::bail!("duplicate winner name: '{}'", msg.winner);
        }

        self.winners.insert(msg.winner, msg.recipient);
        self.broadcast_state();

        Ok(self.deref().into())
    }
}

impl Handler<messages::client::Leave> for Room {
    type Result = anyhow::Result<()>;

    fn handle(&mut self, msg: messages::client::Leave, _ctx: &mut Self::Context) -> Self::Result {
        if !self.winners.contains_key(&msg.winner) {
            anyhow::bail!("cannot remove unknown winner '{}'", msg.winner);
        }

        self.winners.remove(&msg.winner);
        self.broadcast_state();

        Ok(())
    }
}

impl Handler<messages::client::RestartVote> for Room {
    type Result = anyhow::Result<()>;

    fn handle(
        &mut self,
        _msg: messages::client::RestartVote,
        _ctx: &mut Self::Context,
    ) -> Self::Result {
        match &mut self.voting_state {
            RoomVotingState::Idle => {
                anyhow::bail!("no active vote")
            }
            RoomVotingState::Voting(vote) => {
                anyhow::bail!("already voting on '{}'", vote.story.title)
            }
            RoomVotingState::Fighting(fight) => {
                self.voting_state = RoomVotingState::Voting(ActiveVote {
                    participants: fight
                        .participants
                        .keys()
                        .cloned()
                        .map(|winner| (winner, None))
                        .collect(),
                    story: fight.story.clone(),
                })
            }
        }

        self.broadcast_state();

        Ok(())
    }
}

impl Handler<messages::client::StartVote> for Room {
    type Result = anyhow::Result<()>;

    fn handle(
        &mut self,
        msg: messages::client::StartVote,
        ctx: &mut Self::Context,
    ) -> Self::Result {
        match &self.voting_state {
            RoomVotingState::Idle => {}
            RoomVotingState::Voting(vote) => {
                anyhow::bail!("already voting on '{}'", vote.story.title)
            }
            RoomVotingState::Fighting(fight) => {
                anyhow::bail!("already voting on '{}'", fight.story.title)
            }
        }

        self.voting_state = RoomVotingState::Voting(ActiveVote {
            participants: self
                .winners
                .keys()
                .map(|name| (name.clone(), None))
                .collect(),
            story: msg.story,
        });

        self.broadcast_state();

        Ok(())
    }
}

impl Handler<messages::client::Vote> for Room {
    type Result = anyhow::Result<()>;

    fn handle(&mut self, msg: messages::client::Vote, ctx: &mut Self::Context) -> Self::Result {
        let vote = match &mut self.voting_state {
            RoomVotingState::Idle => {
                anyhow::bail!("no voting session active")
            }
            RoomVotingState::Fighting(_) => {
                anyhow::bail!("in fighting state")
            }
            RoomVotingState::Voting(vote) => vote,
        };

        if vote.story.id != msg.story_id {
            anyhow::bail!("voting on wrong story")
        }

        if let Some(winner_vote) = vote.participants.get_mut(&msg.winner) {
            *winner_vote = Some(msg.points);
        } else {
            anyhow::bail!("winner is not a participant")
        }

        // Check if the vote is over (all votes are in)
        if !vote.participants.values().all(Option::is_some) {
            // Nothing to do
        } else {
            let participants: HashMap<_, _> = vote
                .participants
                .iter()
                .map(|(winner, vote)| (winner.clone(), vote.unwrap()))
                .collect();
            let distinct_votes: HashSet<_> = participants.values().copied().collect();

            // If there is one answer, use that as the result
            if distinct_votes.len() == 1 {
                self.past_votes.insert(
                    vote.story.clone(),
                    distinct_votes.into_iter().next().unwrap(),
                );
                self.voting_state = RoomVotingState::Idle;
            } else {
                // If there are more disinct values, get the maximum and minimum
                let min_points = *distinct_votes.iter().min().unwrap();
                let max_points = *distinct_votes.iter().max().unwrap();

                // Select a random winner that voted for these values
                let select_winner_with_vote = |target_points: StoryPoints| {
                    let mut rng = rand::thread_rng();
                    participants
                        .iter()
                        .filter_map(|(winner, points)| {
                            if *points == target_points {
                                Some(winner.clone())
                            } else {
                                None
                            }
                        })
                        .choose(&mut rng)
                };
                let min_vote_participants = select_winner_with_vote(min_points).unwrap();
                let max_vote_participants = select_winner_with_vote(max_points).unwrap();

                self.voting_state = RoomVotingState::Fighting(Fight {
                    fighters: vec![
                        (min_vote_participants, min_points),
                        (max_vote_participants, max_points),
                    ],
                    participants,
                    story: vote.story.clone(),
                });
            }
        }

        self.broadcast_state();

        Ok(())
    }
}

impl Room {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            leader: None,
            past_votes: Default::default(),
            winners: Default::default(),
            voting_state: Default::default(),
        }
    }

    /// Broadcast an update to all subscribed winners
    fn broadcast_state(&self) {
        for (_, client) in self.winners.iter() {
            let _ = client.do_send(RoomStateChanged { state: self.into() });
            // TODO: If failure, remove the client
        }
    }
}

fn main() {
    env_logger::init();
}

#[cfg(test)]
mod test {
    use crate::messages::client;
    use crate::messages::server::RoomStateChanged;
    use crate::{Room, RoomState, RoomVotingState};
    use actix::{
        Actor, ActorContext, ActorFutureExt, Addr, AsyncContext, Context, ContextFutureSpawner,
        Handler, Message, MessageResult, ResponseActFuture, Running, WrapFuture,
    };
    use winner_server::types::{Story, StoryId, StoryPoints, Winner};

    struct TestClient {
        pub winner: Winner,
        pub state: RoomState,
        pub room: Option<Addr<Room>>,
    }

    impl TestClient {
        pub fn new(name: impl Into<String>) -> Self {
            Self {
                winner: Winner(name.into()),
                state: Default::default(),
                room: None,
            }
        }
    }

    impl Actor for TestClient {
        type Context = Context<Self>;

        /// Called when an actor gets polled the first time.
        fn started(&mut self, ctx: &mut Self::Context) {}

        fn stopping(&mut self, _: &mut Self::Context) -> Running {
            if let Some(room) = &self.room {
                room.do_send(client::Leave {
                    winner: self.winner.clone(),
                });
            }
            Running::Stop
        }
    }

    impl Handler<RoomStateChanged> for TestClient {
        type Result = ();

        fn handle(&mut self, msg: RoomStateChanged, ctx: &mut Self::Context) -> Self::Result {
            self.state = msg.state;
        }
    }

    struct GetStateMessage;

    struct Stop;

    struct ConnectToRoom(Addr<Room>);

    impl Message for GetStateMessage {
        type Result = anyhow::Result<RoomState>;
    }

    impl Message for Stop {
        type Result = ();
    }

    impl Message for ConnectToRoom {
        type Result = anyhow::Result<()>;
    }

    impl Handler<GetStateMessage> for TestClient {
        type Result = anyhow::Result<RoomState>;

        fn handle(&mut self, msg: GetStateMessage, ctx: &mut Self::Context) -> Self::Result {
            Ok(self.state.clone())
        }
    }

    impl Handler<Stop> for TestClient {
        type Result = MessageResult<Stop>;

        fn handle(&mut self, msg: Stop, ctx: &mut Self::Context) -> Self::Result {
            ctx.stop();
            MessageResult(())
        }
    }

    impl Handler<ConnectToRoom> for TestClient {
        type Result = ResponseActFuture<Self, anyhow::Result<()>>;

        fn handle(&mut self, msg: ConnectToRoom, ctx: &mut Self::Context) -> Self::Result {
            // connect to the room
            let addr = ctx.address();
            self.room = Some(msg.0.clone());
            Box::pin(
                msg.0
                    .send(client::Enter {
                        recipient: addr.recipient(),
                        winner: self.winner.clone(),
                    })
                    .into_actor(self)
                    .then(|res, act, ctx| {
                        match res {
                            Ok(Ok(res)) => act.state = res,
                            // something is wrong with chat server
                            _ => ctx.stop(),
                        }
                        actix::fut::ready(Ok(()))
                    }),
            )
        }
    }

    async fn test_setup() -> (Addr<Room>, Addr<TestClient>, Addr<TestClient>) {
        let room = Room::new("Test").start();
        let client = TestClient::new("test1").start();
        let client2 = TestClient::new("test2").start();

        // Connect to the room
        client
            .send(ConnectToRoom(room.clone()))
            .await
            .unwrap()
            .expect("could not connect to room");
        client2
            .send(ConnectToRoom(room.clone()))
            .await
            .unwrap()
            .expect("could not connect to room");

        (room, client, client2)
    }

    #[actix::test]
    async fn enter_and_leave() {
        let (room, client, client2) = test_setup().await;

        let state = client2.send(GetStateMessage).await.unwrap().unwrap();
        assert_eq!(state.winners.len(), 2);
        assert!(state.winners.contains(&Winner(String::from("test1"))));
        assert!(state.winners.contains(&Winner(String::from("test2"))));

        client.send(Stop).await.unwrap();

        let state = client2.send(GetStateMessage).await.unwrap().unwrap();
        assert_eq!(state.winners.len(), 1);
        assert!(state.winners.contains(&Winner(String::from("test2"))));
    }

    #[actix::test]
    async fn vote_on_story() {
        let (room, client, ..) = test_setup().await;

        let story = Story::new(StoryId(1), "Bla");
        room.send(crate::messages::client::StartVote {
            story: story.clone(),
        })
        .await
        .unwrap()
        .unwrap();

        let state = client.send(GetStateMessage).await.unwrap().unwrap();
        matches!(state.voting_state, RoomVotingState::Voting(_));

        // Vote for both winners
        for winner in state.winners {
            room.send(crate::messages::client::Vote {
                winner,
                story_id: story.id.clone(),
                points: StoryPoints::ONE,
            })
            .await
            .unwrap()
            .unwrap();
        }

        // Same points so should return to idle
        let state = client.send(GetStateMessage).await.unwrap().unwrap();
        matches!(state.voting_state, RoomVotingState::Idle);
    }
}
