use anyhow::Error;
use futures::AsyncBufReadExt;
use rand::random;
use std::{env, thread};
use tokio::sync::mpsc::Receiver;
use winner_actor::codec::ClientWinnerCodec;
use winner_actor::messages::client::ClientRequest;
use winner_actor::room::{RoomState, RoomVotingState};
use winner_cli;
use winner_cli::ClientAPI;
use winner_server::types::{Story, StoryId, Winner};

fn print_room_state(winner: Winner, state: RoomState) {
    println!("{}", state);
    list_options(winner, state.voting_state);
}

fn list_options(winner: Winner, voting_state: RoomVotingState) {
    match voting_state {
        RoomVotingState::Idle => {
            println!("Idle state, options: 'enter name to start vote on that story'");
        }
        RoomVotingState::Voting(active_vote) => {
            if active_vote.participants[&winner].is_some() {
                println!("New vote is in, you've voted");
            } else {
                println!(
                    "Voting on story {}, enter Story Points: (1, 2, 3, 5, 8, 13, 20)",
                    active_vote.story.title
                );
            }
        }
        RoomVotingState::Fighting(fight) => {
            // List fighters
            println!("Fighting:");
            for (winner, points) in &fight.fighters {
                println!("{} with {:?}", winner.0, points);
            }
            println!("When ready restart vote by entering: (1)")
        }
    }
}

/// Either a new state of an input
#[derive(Debug)]
enum Action {
    NewState(RoomState),
    Input(String),
    ServerShutdown,
}

fn select_between_input(
    mut receiver: Receiver<RoomState>,
    mut console_msg: Receiver<String>,
) -> Receiver<Action> {
    let (tx_action, rx_action) = tokio::sync::mpsc::channel(100);
    tokio::spawn(async move {
        loop {
            let value = tokio::select! {
                message = receiver.recv() => { if let Some(state) = message { Action::NewState(state) } else { Action::ServerShutdown }  }
                line = console_msg.recv() => { if let Some(line) = line { Action::Input(line)} else { Action::ServerShutdown }  }
            };
            tx_action
                .send(value)
                .await
                .expect("Could not send over channel");
        }
    });

    rx_action
}

fn console_loop() -> tokio::sync::mpsc::Receiver<String> {
    let (io_tx, io_rx) = tokio::sync::mpsc::channel(100);
    thread::spawn(move || loop {
        let mut cmd = String::new();
        if std::io::stdin().read_line(&mut cmd).is_err() {
            println!("error");
            return;
        }

        io_tx
            .blocking_send(cmd)
            .expect("Could not send std message");
    });
    io_rx
}

#[tokio::main]
async fn main() {
    // Get the address
    let winner_name = env::args().nth(1).expect("Please enter a winner name");

    // Get the winner name
    let winner = Winner(winner_name);

    // Get the address
    let addr = env::args()
        .nth(2)
        .unwrap_or_else(|| "127.0.0.1:8000".to_string());

    // Connect to server
    let tcp_stream = tokio::net::TcpStream::connect(addr)
        .await
        .expect("Could not connect to server");

    // Connect the client
    let (mut client, mut receiver) = winner_cli::create_client(tcp_stream, winner.clone()).await;
    // Enter
    client.enter_room().await.expect("Could not enter room");
    println!("Entered room");
    let mut last_room_state = receiver
        .recv()
        .await
        .expect("Could not receive first server state");

    // Setup receiving of input
    let console_receiver = console_loop();

    // Create state and input receiver
    let mut action_receiver = select_between_input(receiver, console_receiver);

    // Receive actions
    while let Some(action) = action_receiver.recv().await {
        match action {
            Action::NewState(state) => {
                last_room_state = state.clone();
                print_room_state(winner.clone(), state);
            }
            Action::Input(user_input) => {
                handle_input(user_input, &last_room_state, &mut client).await;
            }
            Action::ServerShutdown => break,
        }
    }
}

fn random_story_id(room_state: &RoomState) -> usize {
    let mut random: usize = rand::random();
    while room_state
        .past_votes
        .keys()
        .any(|story| story.id.0 == random)
    {
        random = rand::random();
    }
    random
}

fn handle_result(result: anyhow::Result<()>) {
    match result {
        Ok(_) => {}
        Err(e) => {
            println!("{}", e)
        }
    }
}

/// Handle user input
async fn handle_input(user_input: String, room_state: &RoomState, client: &mut ClientAPI) {
    match &room_state.voting_state {
        RoomVotingState::Idle => {
            // Start a vote
            handle_result(
                client
                    .start_vote(Story::new(StoryId(random_story_id(room_state)), user_input))
                    .await,
            );
        }
        RoomVotingState::Voting(vote) => {}
        RoomVotingState::Fighting(_) => {}
    }
}
