use actix::prelude::*;
use std::collections::HashMap;
use winner_server::messages::{RoomStateChange, StateChange};
use winner_server::types::{Story, StoryPoints, Winner};

pub type ClientMessages = winner_server::messages::ClientMessages;

pub mod client {
    use super::server;
    use crate::RoomState;
    use actix::{Addr, Message, Recipient};
    use winner_server::messages::RoomInitialState;
    use winner_server::types::{Story, StoryId, StoryPoints, Winner};

    // /// Want to enter the room
    // EnterRoom(Winner),
    // /// Leave the room
    // LeaveRoom(Winner),
    // /// Reply to acknowledge the leader
    // AcknowledgeLeader(bool),
    // /// Sent when a vote has to be done for a story
    // StartVote(Story),
    // /// Sent to cast an actual vote
    // Vote((Winner, Story, StoryPoints)),
    // /// Fight resolved
    // FightResolved,
    // // Final story points, which should include the leader correct story and storypoints
    // //FinalStoryPoints((Winner, Story, StoryPoints)),

    /// Sent by the client when it wants to enter a room
    pub struct Enter {
        pub winner: Winner,
        pub recipient: Recipient<server::RoomStateChanged>,
    }

    impl Message for Enter {
        type Result = anyhow::Result<RoomState>;
    }

    /// Sent by the client when it leaves a room
    pub struct Leave {
        pub winner: Winner,
    }

    impl Message for Leave {
        type Result = anyhow::Result<()>;
    }

    /// Start voting on a story
    pub struct StartVote {
        pub story: Story,
    }

    impl Message for StartVote {
        type Result = anyhow::Result<()>;
    }

    /// Restart voting on a story after a fight
    pub struct RestartVote {}

    impl Message for RestartVote {
        type Result = anyhow::Result<()>;
    }

    /// Cast your vote on a story
    pub struct Vote {
        pub winner: Winner,
        pub story_id: StoryId,
        pub points: StoryPoints,
    }

    impl Message for Vote {
        type Result = anyhow::Result<()>;
    }
}

pub mod server {
    use crate::RoomState;
    use actix::Message;

    pub struct RoomStateChanged {
        pub state: RoomState,
    }

    impl Message for RoomStateChanged {
        type Result = ();
    }
}
