pub mod client {
    use super::server;
    use crate::room::RoomState;
    use actix::prelude::*;
    use actix::Recipient;
    use serde::{Deserialize, Serialize};
    use winner_server::types::{Story, StoryId, StoryPoints, Winner};

    #[derive(Serialize, Deserialize)]
    #[serde(tag = "cmd", content = "data")]
    pub enum ClientRequest {
        Enter(Winner),
        Leave(Winner),
        StartVote(Story),
        RestartVote,
        Vote(Winner, StoryId, StoryPoints),
    }

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
    use crate::room::RoomState;
    use actix::Message;
    use serde::{Deserialize, Serialize};

    #[derive(Serialize, Deserialize)]
    pub enum ServerResponse {
        Ok,
        Err(String),
        State(RoomState),
        Ping,
    }

    #[derive(Serialize, Deserialize)]
    pub struct RoomStateChanged {
        pub state: RoomState,
    }

    impl Message for RoomStateChanged {
        type Result = ();
    }
}
