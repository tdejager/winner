#![allow(dead_code)]
use std::io;

use crate::messages::client::ClientRequest;
use crate::messages::server::ServerResponse;
use actix_codec::{Decoder, Encoder};
use byteorder::{BigEndian, ByteOrder};
use bytes::{BufMut, BytesMut};
use serde_json as json;

/// Codec for server side
pub struct WinnerCodec;

impl Decoder for WinnerCodec {
    type Item = crate::messages::client::ClientRequest;
    type Error = io::Error;

    fn decode(&mut self, src: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        let size = {
            if src.len() < 2 {
                return Ok(None);
            }
            BigEndian::read_u16(src.as_ref()) as usize
        };

        if src.len() >= size + 2 {
            let _ = src.split_to(2);
            let buf = src.split_to(size);
            Ok(Some(json::from_slice::<ClientRequest>(&buf)?))
        } else {
            Ok(None)
        }
    }
}

impl Encoder<ServerResponse> for WinnerCodec {
    type Error = io::Error;

    fn encode(&mut self, msg: ServerResponse, dst: &mut BytesMut) -> Result<(), Self::Error> {
        let msg = json::to_string(&msg).unwrap();
        let msg_ref: &[u8] = msg.as_ref();

        dst.reserve(msg_ref.len() + 2);
        dst.put_u16(msg_ref.len() as u16);
        dst.put(msg_ref);

        Ok(())
    }
}

/// Codec for client side
pub struct ClientChatCodec;

impl Decoder for ClientChatCodec {
    type Item = ServerResponse;
    type Error = io::Error;

    fn decode(&mut self, src: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        let size = {
            if src.len() < 2 {
                return Ok(None);
            }
            BigEndian::read_u16(src.as_ref()) as usize
        };

        if src.len() >= size + 2 {
            let _ = src.split_to(2);
            let buf = src.split_to(size);
            Ok(Some(json::from_slice::<ServerResponse>(&buf)?))
        } else {
            Ok(None)
        }
    }
}

impl Encoder<ClientRequest> for ClientChatCodec {
    type Error = io::Error;

    fn encode(&mut self, msg: ClientRequest, dst: &mut BytesMut) -> Result<(), Self::Error> {
        let msg = json::to_string(&msg).unwrap();
        let msg_ref: &[u8] = msg.as_ref();

        dst.reserve(msg_ref.len() + 2);
        dst.put_u16(msg_ref.len() as u16);
        dst.put(msg_ref);

        Ok(())
    }
}
