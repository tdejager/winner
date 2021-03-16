use std::sync::Arc;

use serde_json::Value;
use tokio::{
    net::tcp::{OwnedReadHalf, OwnedWriteHalf},
    sync::Mutex,
};
use tokio_serde::formats::SymmetricalJson;
use tokio_util::codec::{FramedRead, FramedWrite, LengthDelimitedCodec};

/// This represents a message stream
pub type MessageStreamRead = tokio_serde::SymmetricallyFramed<
    FramedRead<OwnedReadHalf, LengthDelimitedCodec>,
    Value,
    SymmetricalJson<Value>,
>;

/// This represents a message stream
pub type MessageStreamWrite = tokio_serde::SymmetricallyFramed<
    FramedWrite<OwnedWriteHalf, LengthDelimitedCodec>,
    Value,
    SymmetricalJson<Value>,
>;

/// Shared write stream
pub type SharedStreamWrite = Arc<Mutex<MessageStreamWrite>>;
pub type SharedStreamRead = Arc<Mutex<MessageStreamRead>>;

/// Create a stream that is able to read json messages from a tcp stream
pub fn create_read_stream(read_half: OwnedReadHalf) -> MessageStreamRead {
    let length_delimited_read = FramedRead::new(read_half, LengthDelimitedCodec::new());
    MessageStreamRead::new(length_delimited_read, SymmetricalJson::<Value>::default())
}

/// Create a shared read stream
pub fn create_shared_read_stream(read_half: OwnedReadHalf) -> SharedStreamRead {
    Arc::new(Mutex::new(create_read_stream(read_half)))
}

/// Create a stream that is able to read json messages from a tcp stream
pub fn create_write_stream(write_half: OwnedWriteHalf) -> MessageStreamWrite {
    let length_delimited_write = FramedWrite::new(write_half, LengthDelimitedCodec::new());
    MessageStreamWrite::new(length_delimited_write, SymmetricalJson::<Value>::default())
}

/// Create a shared write stream that can be shared over threads
pub fn create_shared_write_stream(write_half: OwnedWriteHalf) -> SharedStreamWrite {
    Arc::new(Mutex::new(create_write_stream(write_half)))
}
