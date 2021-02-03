use crate::types::{Story, StoryPoints, Winner};
use serde_derive::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum StateChange {
    ENTER,
    LEAVE,
    LEADER,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct RoomStateChangeMessage {
    pub change: StateChange,
    pub winner: Winner,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct FightMessage {
    pub winner_1: Winner,
    pub winner_2: Winner,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct StartVoteMessage {
    pub story: Story,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
/// Messages that are sent by the server
pub enum ServerMessages {
    /// Sent when something in a room changes
    RoomStateChange(RoomStateChangeMessage),
    /// Sent when two people should debate story points
    Fight(FightMessage),
    /// Sent when a vote has to be done for a story
    StartVote(StartVoteMessage),
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct VoteMessage {
    pub story: Story,
    pub story_points: StoryPoints,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
/// Messages that are received by the server
pub enum ClientMessages {
    RoomStateChange(RoomStateChangeMessage),
    /// Reply to acknowledge the leader
    AcknowledgeLeader(bool),
    /// Sent when a vote has to be done for a story
    StartVote(StartVoteMessage),
    /// Sent to cast an actual vote
    Vote(VoteMessage),
}
