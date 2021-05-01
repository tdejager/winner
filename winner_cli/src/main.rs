use std::{env, sync::Arc, thread};
use tokio::sync::mpsc::Receiver;
use tokio::sync::Mutex;
use winner_actor::room::{RoomState, RoomVotingState};
use winner_cli;
use winner_cli::ClientAPI;
use winner_server::types::{Story, StoryId, StoryPoints, Winner};

/// Print the state of the room
fn print_room_state(state: &RoomState) {
    println!("{}", state);
}

// Run the console loop
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

/// Generate a random story id
fn random_story_id(room_state: &RoomState) -> StoryId {
    let mut random: usize = rand::random();
    while room_state
        .past_votes
        .keys()
        .any(|story| story.id.0 == random)
    {
        random = rand::random();
    }
    StoryId(random)
}

/// Print the result
fn handle_result(result: anyhow::Result<()>) {
    match result {
        Ok(_) => {}
        Err(e) => {
            println!("{}", e)
        }
    }
}

#[tokio::main]
async fn main() {
    // Prompt for winner name
    let winner_name = env::args().nth(1).expect("Please enter a winner name:");

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

    // Setup receiving of input
    let mut last_room_state: Option<RoomState> = None;
    let console_receiver = Arc::new(Mutex::new(console_loop()));

    // Set the client in a mutex
    let client = Arc::new(Mutex::new(client));

    // Create state and input receiver
    while let Some(state) = receiver.recv().await {
        print_room_state(&state);

        // Spawn a new input handler if required
        if last_room_state.is_none() || last_room_state.unwrap().voting_state != state.voting_state
        {
            // Spawn input handler, this asks the questions to the user and handles the input
            spawn_input(&state, console_receiver.clone(), client.clone());
        }
        last_room_state = Some(state.clone());
    }
}

/// Create a task here that can get input from the console and return a server call
fn spawn_input(
    room_state: &RoomState,
    console_receiver: Arc<Mutex<Receiver<String>>>,
    client_api: Arc<Mutex<ClientAPI>>,
) -> tokio::task::JoinHandle<()> {
    let cloned = room_state.clone();
    tokio::spawn(async move { input_loop(cloned, console_receiver, client_api).await })
}

/// Run the actual input loop
async fn input_loop(
    room_state: RoomState,
    console_receiver: Arc<Mutex<Receiver<String>>>,
    client_api: Arc<Mutex<ClientAPI>>,
) {
    match &room_state.voting_state {
        // When we are in an idle state
        RoomVotingState::Idle => {
            async move {
                loop {
                    println!("Options: 1) Enter a 'Story name' name to vote");
                    let option = console_receiver.lock().await.recv().await.unwrap();
                    if !option.eq("1") {
                        continue;
                    }
                    println!("Enter a story name: ");
                    let name = console_receiver.lock().await.recv().await.unwrap();
                    // Try to start an actual vote
                    let result = client_api
                        .lock()
                        .await
                        .start_vote(Story::new(random_story_id(&room_state), name))
                        .await;
                    handle_result(result);
                }
            }
            .await;
        }
        // When we are voting
        RoomVotingState::Voting(vote) => {
            async move {
                loop {
                    println!("Voting on story {}", vote.story.title);
                    println!("Cast your vote (1, 2, 3, 5, 8, 13, 20): ");
                    let points: StoryPoints = console_receiver
                        .lock()
                        .await
                        .recv()
                        .await
                        .unwrap()
                        .parse()
                        .unwrap();
                    println!("You've voted with {}, points", points);
                    let result = client_api
                        .lock()
                        .await
                        .vote(vote.story.id.clone(), points)
                        .await;
                    handle_result(result);
                }
            }
            .await;
        }
        // When we are fighting
        RoomVotingState::Fighting(fight) => {
            // Create printed list of fighters
            let fighters_str = fight
                .participants
                .iter()
                .map(|(w, p)| format!("{} with {}", w, p))
                .collect::<Vec<String>>()
                .join(", ");
            println!("Current fighters '{}'", fighters_str);
            println!("Cast your vote (1, 2, 3, 5, 8, 13, 20): ");
            let points: StoryPoints = console_receiver
                .lock()
                .await
                .recv()
                .await
                .unwrap()
                .parse()
                .unwrap();
            println!("You've voted with {}, points", points);
            // Recast vote
            let result = client_api
                .lock()
                .await
                .vote(fight.story.id.clone(), points)
                .await;
            handle_result(result);
        }
    }
}
