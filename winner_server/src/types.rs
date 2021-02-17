use serde_derive::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, Hash)]
/// The people that we call winners that are in the room
pub struct Winner(pub String);

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
pub struct StoryId(pub usize);

#[derive(Serialize, Deserialize, Debug, Clone, Eq, PartialEq)]
pub struct Story {
    pub id: StoryId,
    pub title: String,
}

impl Story {
    pub fn new<T: Into<String>>(id: StoryId, title: T) -> Self {
        Story {
            id,
            title: title.into(),
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, Ord, PartialOrd, Eq, PartialEq)]
pub enum StoryPoints {
    ONE = 1,
    TWO = 2,
    THREE = 3,
    FIVE = 5,
    EIGHT = 8,
    THIRTEEN = 13,
    TWENTY = 20,
    UNKOWN = 100,
    COFFEE = 101,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Vote {
    pub story_count: StoryPoints,
}
