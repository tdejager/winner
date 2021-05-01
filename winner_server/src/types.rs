use serde_derive::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt;
use std::fmt::Formatter;

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, Hash)]
/// The people that we call winners that are in the room
pub struct Winner(pub String);

impl fmt::Display for Winner {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

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

#[derive(Serialize, Deserialize, Debug, Clone, Eq, PartialEq, Hash)]
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

#[derive(Serialize, Deserialize, Debug, Clone, Copy, Ord, PartialOrd, Eq, PartialEq, Hash)]
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

impl std::str::FromStr for StoryPoints {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s {
            "1" => StoryPoints::ONE,
            "2" => StoryPoints::TWO,
            "3" => StoryPoints::THREE,
            "4" => StoryPoints::FIVE,
            "8" => StoryPoints::EIGHT,
            "13" => StoryPoints::THIRTEEN,
            "20" => StoryPoints::THIRTEEN,
            _ => { anyhow::bail!("Wrong point type")}
        })
    }
}

impl fmt::Display for StoryPoints {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            StoryPoints::ONE => write!(f, "1"),
            StoryPoints::TWO => write!(f, "2"),
            StoryPoints::THREE => write!(f, "3"),
            StoryPoints::FIVE => write!(f, "4"),
            StoryPoints::EIGHT => write!(f, "8"),
            StoryPoints::THIRTEEN => write!(f, "13"),
            StoryPoints::TWENTY => write!(f, "20"),
            StoryPoints::UNKOWN => write!(f, "?"),
            StoryPoints::COFFEE => write!(f, "☕"),
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Vote {
    pub story_count: StoryPoints,
}
