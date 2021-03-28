use crate::types::{Story, StoryPoints, Winner};
use serde_derive::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Serialize, Deserialize, Debug, Clone, Eq, PartialEq)]
pub enum StateChange {
    Enter,
    Leave,
    Leader,
}

#[derive(Serialize, Deserialize, Debug, Clone, Eq, PartialEq)]
pub enum RoomStateChange {
    Voting,
    Idle,
}

#[derive(Serialize, Deserialize, Clone, Debug, Eq, PartialEq)]
pub struct RoomInitialState {
    pub winners: Vec<Winner>,
    pub leader: Option<Winner>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
/// Messages that are sent by the server
pub enum ServerMessages {
    /// Sent when something in a room changes
    RoomParticipantsChange((Winner, StateChange)),
    /// Sent when the state of the room changes
    RoomStateChange(RoomStateChange),
    /// Sent when two people should debate story points
    Fight((Winner, Winner)),
    /// Sent when the fight has been resolved
    FightResolved,
    /// Sent when a vote has been cast
    VoteCast((Winner, StoryPoints)),
    /// Sent when a vote has to be done for a story
    StartVote(Story),
    /// Vote has finished
    VotesReceived(HashMap<Winner, StoryPoints>),
    /// Initial state
    InitialState(RoomInitialState),
    /// Ok Response to a ClientMessage request
    ServerOk(),
    /// Err Response to a ClientMessage request
    ServerErr(String),
}

#[derive(Serialize, Deserialize, Debug, Clone)]
/// Messages that are received by the server
pub enum ClientMessages {
    /// Want to enter the room
    EnterRoom(Winner),
    /// Leave the room
    LeaveRoom(Winner),
    /// Reply to acknowledge the leader
    AcknowledgeLeader(bool),
    /// Sent when a vote has to be done for a story
    StartVote(Story),
    /// Sent to cast an actual vote
    Vote((Winner, Story, StoryPoints)),
    /// Fight resolved
    FightResolved,
    // Final story points, which should include the leader correct story and storypoints
    //FinalStoryPoints((Winner, Story, StoryPoints)),
}
