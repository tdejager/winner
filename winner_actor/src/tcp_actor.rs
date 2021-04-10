use crate::codec::WinnerCodec;
use crate::messages::client::ClientRequest;
use crate::messages::server::{RoomStateChanged, ServerResponse};
use crate::room::Room;
use actix::prelude::*;
use futures::future::Ready;
use futures::io::Error;
use futures::StreamExt;
use std::str::FromStr;
use std::time::{Duration, Instant};
use tokio::io::WriteHalf;
use tokio::net::{TcpListener, TcpStream};
use tokio_util::codec::FramedRead;
use winner_server::types::Winner;

pub struct RoomSession {
    winner: Winner,
    room_addr: Addr<Room>,
    framed: actix::io::FramedWrite<ServerResponse, WriteHalf<TcpStream>, WinnerCodec>,
    // last_heart_beat: Instant,
}

impl Actor for RoomSession {
    /// For tcp communication we are going to use `FramedContext`.
    /// It is convenient wrapper around `Framed` object from `tokio_io`
    type Context = Context<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {}

    fn stopping(&mut self, _: &mut Self::Context) -> Running {
        // notify chat server
        self.room_addr.do_send(crate::messages::client::Leave {
            winner: self.winner.clone(),
        });
        Running::Stop
    }
}

impl RoomSession {
    pub fn new(
        addr: Addr<Room>,
        framed: actix::io::FramedWrite<ServerResponse, WriteHalf<TcpStream>, WinnerCodec>,
    ) -> Self {
        Self {
            winner: Winner("INVALID".into()),
            room_addr: addr,
            framed,
            // last_heart_beat: Instant::now(),
        }
    }

    /// Handle the result
    pub fn handle_room_result(&mut self, result: anyhow::Result<()>) {
        match result {
            Ok(_) => self.framed.write(ServerResponse::Ok),
            Err(msg) => self.framed.write(ServerResponse::Err(format!("{}", msg))),
        }
    }

    // /// Sends heartbeat to client and stops actor when it stops responding
    // fn heart_beat(&self, ctx: &mut Context<Self>) {
    //     ctx.run_interval(Duration::new(1, 0), |act, ctx| {
    //         // check client heartbeats
    //         if Instant::now().duration_since(act.last_heart_beat) > Duration::new(10, 0) {
    //             // heartbeat timed out
    //             println!("Client heartbeat failed, disconnecting!");
    //
    //             // TODO don't think this is needed
    //             // notify chat server
    //             //act.addr.do_send(crate::messages::server:: { id: act.id });
    //
    //             // stop actor
    //             ctx.stop();
    //         }
    //
    //         act.framed
    //             .write(crate::messages::server::ServerResponse::Ping);
    //         // if we can not send message to sink, sink is closed (disconnected)
    //     });
    // }
}

fn handle_room_response(
    res: anyhow::Result<anyhow::Result<()>, MailboxError>,
    act: &mut RoomSession,
    ctx: &mut Context<RoomSession>,
) -> Ready<()> {
    match res {
        Ok(inner_result) => act.handle_room_result(inner_result),
        // Something is wrong with the server
        _ => ctx.stop(),
    };
    actix::fut::ready(())
}

impl StreamHandler<Result<ClientRequest, tokio::io::Error>> for RoomSession {
    fn handle(&mut self, message: Result<ClientRequest, Error>, ctx: &mut Self::Context) {
        // TODO split into subscribed to room and unsubscribed state
        // So that we can deal with a connected client that is not connected to the room yet
        match message {
            Ok(request) => match request {
                ClientRequest::Enter(winner) => {
                    // register self in chat server. `AsyncContext::wait` register
                    // future within context, but context waits until this future resolves
                    // before processing any other events.
                    let addr = ctx.address();
                    // Set new winner name
                    self.winner = winner;
                    self.room_addr
                        .send(crate::messages::client::Enter {
                            winner: self.winner.clone(),
                            recipient: addr.recipient(),
                        })
                        .into_actor(self)
                        .then(|res, act, ctx| {
                            match res {
                                Ok(state) => {
                                    if let Ok(_) = state {
                                        act.handle_room_result(Ok(()));
                                    } else {
                                        ctx.stop()
                                    }
                                }
                                // something is wrong with chat server
                                _ => ctx.stop(),
                            }
                            actix::fut::ready(())
                        })
                        .wait(ctx);
                }
                ClientRequest::StartVote(story) => self
                    .room_addr
                    .send(crate::messages::client::StartVote { story })
                    .into_actor(self)
                    .then(&handle_room_response)
                    .wait(ctx),
                ClientRequest::RestartVote => self
                    .room_addr
                    .send(crate::messages::client::RestartVote {})
                    .into_actor(self)
                    .then(&handle_room_response)
                    .wait(ctx),
                ClientRequest::Vote(winner, story_id, points) => self
                    .room_addr
                    .send(crate::messages::client::Vote {
                        winner,
                        story_id,
                        points,
                    })
                    .into_actor(self)
                    .then(&handle_room_response)
                    .wait(ctx),
                _ => self.handle_room_result(Err(anyhow::anyhow!("Wrong request received"))),
            },
            // Error in decoding
            Err(_) => ctx.stop(),
        }
    }
}

impl Handler<RoomStateChanged> for RoomSession {
    type Result = ();

    fn handle(&mut self, msg: RoomStateChanged, _ctx: &mut Self::Context) -> Self::Result {
        self.framed.write(ServerResponse::State(msg.state));
    }
}

impl actix::io::WriteHandler<tokio::io::Error> for RoomSession {}

/// Define tcp server that will accept incoming tcp connection and create
/// chat actors.
pub async fn tcp_server(listener: TcpListener, room: Addr<Room>) {
    // Clone address
    let room = room.clone();
    // Start accepting connections
    while let Ok((stream, _)) = listener.accept().await {
        let room = room.clone();
        RoomSession::create(|ctx| {
            let (r, w) = tokio::io::split(stream);
            let reader = FramedRead::new(r, WinnerCodec);
            RoomSession::add_stream(reader, ctx);
            RoomSession::new(room, actix::io::FramedWrite::new(w, WinnerCodec, ctx))
        });
    }
}

#[cfg(test)]
mod test {
    use crate::messages::client::ClientRequest;
    use crate::room;
    use actix::Actor;
    use futures::{SinkExt, StreamExt};
    use tokio::net::{TcpListener, TcpStream};
    use tokio_util::codec::Framed;
    use winner_server::types::Winner;

    #[actix::test]
    pub async fn test_connect_leave() {
        let room = room::Room::new("VotingRoom").start();

        // Setup the TCP side of things
        let tcp_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let tcp_address = tcp_listener.local_addr().unwrap();
        actix::spawn(async { crate::tcp_actor::tcp_server(tcp_listener, room).await });

        // Create a client
        let stream = TcpStream::connect(tcp_address).await.unwrap();
        let mut framed = Framed::new(stream, crate::codec::ClientWinnerCodec);

        let winner = Winner("Me".into());

        // Enter room
        framed.send(ClientRequest::Enter(winner)).await.unwrap();

        dbg!(framed.next().await);
        dbg!(framed.next().await);

        framed.close().await.unwrap();
    }
}
