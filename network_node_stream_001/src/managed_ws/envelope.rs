//! Bounded typed payload carried in the existing opaque channel ABI.
use super::{Error,Result};
use crate::{ws_message_capnp as wire,websocket::{Message,MessageKind}};
use capnp::{message::{Builder,ReaderOptions},serialize};
use sha2::{Digest,Sha256};
pub const VERSION:u16=1;
pub const MAX_ENVELOPE_BYTES:usize=64*1024;
pub fn schema_digest()->[u8;32]{Sha256::digest(include_bytes!("../../schemas/ws_message.capnp")).into()}
impl Message {
    pub fn encode(&self)->Result<Vec<u8>>{
        self.validate(MAX_ENVELOPE_BYTES).map_err(Error::Transport)?;
        let mut message=Builder::new_default();let mut root=message.init_root::<wire::ws_message::Builder>();
        root.set_version(VERSION);root.set_schema_sha256(&schema_digest());
        root.set_kind(match self.kind{MessageKind::Text=>wire::Kind::Text,MessageKind::Binary=>wire::Kind::Binary,
            MessageKind::Ping=>wire::Kind::Ping,MessageKind::Pong=>wire::Kind::Pong,MessageKind::Close=>wire::Kind::Close});
        root.set_payload(&self.payload);root.set_has_close_code(self.close_code.is_some());root.set_close_code(self.close_code.unwrap_or(0));
        let bytes=serialize::write_message_to_words(&message);if bytes.len()>MAX_ENVELOPE_BYTES{return Err(Error::Limit);}Ok(bytes)
    }
    pub fn decode(bytes:&[u8])->Result<Self>{
        if bytes.is_empty()||bytes.len()>MAX_ENVELOPE_BYTES{return Err(Error::Limit);}
        let mut remaining=bytes;let message=serialize::read_message(&mut remaining,ReaderOptions{
            traversal_limit_in_words:Some(2*MAX_ENVELOPE_BYTES/8),nesting_limit:8}).map_err(|_|Error::Invalid)?;
        if !remaining.is_empty(){return Err(Error::Invalid);}
        let root=message.get_root::<wire::ws_message::Reader>().map_err(|_|Error::Invalid)?;
        if root.total_size().map_err(|_|Error::Invalid)?.cap_count!=0||root.get_version()!=VERSION
            ||root.get_schema_sha256().map_err(|_|Error::Invalid)?!=schema_digest()
            ||(!root.get_has_close_code()&&root.get_close_code()!=0){return Err(Error::Invalid);}
        let kind=match root.get_kind().map_err(|_|Error::Invalid)?{wire::Kind::Text=>MessageKind::Text,wire::Kind::Binary=>MessageKind::Binary,
            wire::Kind::Ping=>MessageKind::Ping,wire::Kind::Pong=>MessageKind::Pong,wire::Kind::Close=>MessageKind::Close};
        let payload=root.get_payload().map_err(|_|Error::Invalid)?;if payload.len()>MAX_ENVELOPE_BYTES{return Err(Error::Limit);}
        let result=Self{kind,payload:payload.to_vec(),close_code:root.get_has_close_code().then(||root.get_close_code())};
        result.validate(MAX_ENVELOPE_BYTES).map_err(Error::Transport)?;Ok(result)
    }
}
