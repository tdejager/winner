use serde_derive::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, Hash)]
/// The people that we call winners that are in the room
pub struct Winner(String);

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Room {
    /// Name of the room
    pub name: String,
    /// People participating in the room
    pub winners: Vec<Winner>,
    /// Leader that can start the votes
    pub leader: Winner,
    /// Previously voted stories
    pub votes: HashMap<StoryId, Vec<Vote>>,
}

#[derive(Serialize, Deserialize, PartialEq, Eq, Hash, Debug, Clone)]
pub struct StoryId(usize);

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Story {
    pub id: StoryId,
    pub title: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum StoryPoints {
    ONE,
    TWO,
    THREE,
    FIVE,
    EIGHT,
    THIRTEEN,
    TWENTY,
    EXPLAIN,
    UNKOWN,
    COFFEE,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Vote {
    pub story_count: StoryPoints,
}
