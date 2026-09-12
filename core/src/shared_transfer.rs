//! Private trusted parent/child bootstrap metadata. Neither IDs nor handles are host grants.
//! A reply reports copied bytes and a write probe, never a content transaction or identity proof.
use crate::{Error, Result, shared_object::Descriptor, shared_transfer_capnp as wire};
use capnp::{
    message::{Builder, ReaderOptions},
    serialize,
};
use sha2::{Digest, Sha256};
pub const VERSION: u16 = 1;
pub const MAX_FRAME_BYTES: usize = 128 * 1024;
pub const MAX_PAYLOAD_BYTES: usize = 64 * 1024;
pub fn schema_digest() -> [u8; 32] {
    crate::runtime::schema_digest(include_bytes!("../schemas/shared_transfer.capnp"))
}
fn invalid<T>(_: T) -> Error {
    Error::Invalid("shared transfer message")
}
/// The nonzero handle is meaningful only inside the explicitly intended child process.
/// Do not persist it, resolve it in another process, or treat its number as global identity.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Offer {
    pub transfer: u64,
    pub remote_handle: u64,
    pub descriptor: Descriptor,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Reply {
    pub transfer: u64,
    pub descriptor: Descriptor,
    pub payload: Vec<u8>,
    pub write_rejected: bool,
}
fn message() -> Builder<capnp::message::HeapAllocator> {
    let mut message = Builder::new_default();
    let mut root = message.init_root::<wire::message::Builder>();
    root.set_version(VERSION);
    root.set_schema_digest(&schema_digest());
    message
}
fn finish(message: &Builder<capnp::message::HeapAllocator>) -> Result<Vec<u8>> {
    let bytes = serialize::write_message_to_words(message);
    if bytes.len() > MAX_FRAME_BYTES {
        return Err(Error::Limit);
    }
    Ok(bytes)
}
fn read(bytes: &[u8]) -> Result<capnp::message::Reader<serialize::OwnedSegments>> {
    if bytes.len() > MAX_FRAME_BYTES {
        return Err(Error::Limit);
    }
    let mut rest = bytes;
    // Bounded owned words support transport frames at arbitrary byte alignment.
    let message = serialize::read_message(
        &mut rest,
        ReaderOptions {
            traversal_limit_in_words: Some(MAX_FRAME_BYTES / 8),
            nesting_limit: 8,
        },
    )
    .map_err(invalid)?;
    if !rest.is_empty() {
        return Err(Error::Invalid("trailing shared transfer bytes"));
    }
    let root = message
        .get_root::<wire::message::Reader>()
        .map_err(invalid)?;
    if root.get_version() != VERSION
        || root.get_schema_digest().map_err(invalid)? != schema_digest()
    {
        return Err(Error::UnsupportedVersion);
    }
    Ok(message)
}
impl Offer {
    pub fn validate(&self) -> Result<()> {
        if self.transfer == 0 || self.remote_handle == 0 {
            return Err(Error::Invalid("shared transfer identity"));
        }
        self.descriptor.validate()
    }
    pub fn encode(&self) -> Result<Vec<u8>> {
        self.validate()?;
        let descriptor = self.descriptor.encode()?;
        let mut message = message();
        let root = message
            .get_root::<wire::message::Builder>()
            .map_err(invalid)?;
        let mut offer = root.init_offer();
        offer.set_transfer(self.transfer);
        offer.set_remote_handle(self.remote_handle);
        offer.set_descriptor(&descriptor);
        finish(&message)
    }
    pub fn decode(bytes: &[u8]) -> Result<Self> {
        let message = read(bytes)?;
        let root = message
            .get_root::<wire::message::Reader>()
            .map_err(invalid)?;
        let wire::message::Offer(raw) = root.which().map_err(invalid)? else {
            return Err(Error::Invalid("expected shared transfer offer"));
        };
        let raw = raw.map_err(invalid)?;
        let offer = Self {
            transfer: raw.get_transfer(),
            remote_handle: raw.get_remote_handle(),
            descriptor: Descriptor::decode(raw.get_descriptor().map_err(invalid)?)?,
        };
        offer.validate()?;
        Ok(offer)
    }
}
impl Reply {
    pub fn validate(&self) -> Result<()> {
        if self.transfer == 0 {
            return Err(Error::Invalid("shared transfer identity"));
        }
        if self.payload.is_empty() || self.payload.len() > MAX_PAYLOAD_BYTES {
            return Err(Error::Limit);
        }
        self.descriptor.validate()?;
        if self.descriptor.length != self.payload.len() as u64 {
            return Err(Error::Invalid("shared transfer payload length"));
        }
        if Sha256::digest(&self.payload).as_slice() != self.descriptor.sha256 {
            return Err(Error::Integrity);
        }
        Ok(())
    }
    pub fn encode(&self) -> Result<Vec<u8>> {
        self.validate()?;
        let descriptor = self.descriptor.encode()?;
        let mut message = message();
        let root = message
            .get_root::<wire::message::Builder>()
            .map_err(invalid)?;
        let mut reply = root.init_reply();
        reply.set_transfer(self.transfer);
        reply.set_descriptor(&descriptor);
        reply.set_payload(&self.payload);
        reply.set_write_rejected(self.write_rejected);
        finish(&message)
    }
    pub fn decode(bytes: &[u8]) -> Result<Self> {
        let message = read(bytes)?;
        let root = message
            .get_root::<wire::message::Reader>()
            .map_err(invalid)?;
        let wire::message::Reply(raw) = root.which().map_err(invalid)? else {
            return Err(Error::Invalid("expected shared transfer reply"));
        };
        let raw = raw.map_err(invalid)?;
        let payload = raw.get_payload().map_err(invalid)?;
        if payload.is_empty() || payload.len() > MAX_PAYLOAD_BYTES {
            return Err(Error::Limit);
        }
        let reply = Self {
            transfer: raw.get_transfer(),
            descriptor: Descriptor::decode(raw.get_descriptor().map_err(invalid)?)?,
            payload: payload.to_vec(),
            write_rejected: raw.get_write_rejected(),
        };
        reply.validate()?;
        Ok(reply)
    }
}
