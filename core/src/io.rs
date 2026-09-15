//! IO v1 codec and declaration checks. Decoding grants no file, network, or credential access.
use crate::{Error, Result, identity, io_capnp as wire};
use capnp::{
    message::{Builder, ReaderOptions},
    serialize,
};
use prost::Message;
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;

pub const VERSION: u16 = 1;
pub const FEATURE: &str = "io-v1";
pub const MAX_FRAME_BYTES: usize = 128 * 1024;
pub const MAX_PAYLOAD_BYTES: usize = 64 * 1024;
pub const MAX_CHUNK_BYTES: u32 = 64 * 1024;
pub const MAX_KINDS: usize = 8;

pub mod manifest_proto {
    include!(concat!(env!("OUT_DIR"), "/morrow.plugin.io.v1.rs"));
}

pub fn schema_digest() -> [u8; 32] {
    crate::runtime::schema_digest(include_bytes!("../schemas/io.capnp"))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Kind {
    FileRead,
    FileList,
    FileCreate,
    FileReplace,
    FileDelete,
    HttpGet,
    HttpSend,
    CredentialUse,
}

impl Kind {
    pub fn effectful(self) -> bool {
        matches!(
            self,
            Self::FileCreate | Self::FileReplace | Self::FileDelete | Self::HttpSend
        )
    }

    fn from_manifest(value: manifest_proto::IoKind) -> Option<Self> {
        Some(match value {
            manifest_proto::IoKind::FileRead => Self::FileRead,
            manifest_proto::IoKind::FileList => Self::FileList,
            manifest_proto::IoKind::FileCreate => Self::FileCreate,
            manifest_proto::IoKind::FileReplace => Self::FileReplace,
            manifest_proto::IoKind::FileDelete => Self::FileDelete,
            manifest_proto::IoKind::HttpGet => Self::HttpGet,
            manifest_proto::IoKind::HttpSend => Self::HttpSend,
            manifest_proto::IoKind::CredentialUse => Self::CredentialUse,
            manifest_proto::IoKind::Unspecified => return None,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Declaration {
    pub requested: BTreeSet<Kind>,
    pub effectful_handlers: Vec<String>,
}

pub fn decode_declaration(bytes: &[u8], expected_schema: &[u8]) -> Result<Declaration> {
    if bytes.is_empty() {
        return Err(Error::Invalid("missing io declaration"));
    }
    if bytes.len() > 16 * 1024 {
        return Err(Error::Limit);
    }
    let declared = manifest_proto::IoDeclaration::decode(bytes)
        .map_err(|_| Error::Invalid("io declaration protobuf"))?;
    if declared.schema_version != 1 || declared.schema_sha256 != expected_schema {
        return Err(Error::UnsupportedVersion);
    }
    if declared.requested.len() > MAX_KINDS {
        return Err(Error::Limit);
    }
    let mut requested = BTreeSet::new();
    for raw in declared.requested {
        let kind = manifest_proto::IoKind::try_from(raw)
            .ok()
            .and_then(Kind::from_manifest)
            .ok_or(Error::UnsupportedVersion)?;
        if !requested.insert(kind) {
            return Err(Error::Invalid("duplicate io kind"));
        }
    }
    if requested.is_empty() {
        return Err(Error::Invalid("empty io declaration"));
    }
    let mut handlers = Vec::new();
    for name in declared.effectful_handlers {
        identity(&name)?;
        handlers.push(name);
    }
    if let Some(budget) = declared.budget {
        if budget.max_chunk_bytes == 0
            || budget.max_chunk_bytes > MAX_CHUNK_BYTES
            || budget.max_resources == 0
            || budget.max_resources > 8
            || budget.max_jobs == 0
            || budget.max_jobs > 4
            || budget.max_job_bytes == 0
            || budget.max_job_bytes > 16 * 1024 * 1024
            || budget.max_instance_bytes == 0
            || budget.max_instance_bytes > 64 * 1024 * 1024
            || budget.max_seconds == 0
            || budget.max_seconds > 30
        {
            return Err(Error::Limit);
        }
    }
    Ok(Declaration {
        requested,
        effectful_handlers: handlers,
    })
}

fn invalid<T>(_: T) -> Error {
    Error::Invalid("io frame")
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
    let mut remaining = bytes;
    let message = serialize::read_message(
        &mut remaining,
        ReaderOptions {
            traversal_limit_in_words: Some(2 * MAX_FRAME_BYTES / 8),
            nesting_limit: 8,
        },
    )
    .map_err(invalid)?;
    if !remaining.is_empty() {
        return Err(invalid(()));
    }
    let root = message.get_root::<wire::message::Reader>().map_err(invalid)?;
    root.total_size().map_err(invalid)?;
    if root.get_version() != VERSION || root.get_schema_digest().map_err(invalid)? != schema_digest()
    {
        return Err(Error::UnsupportedVersion);
    }
    Ok(message)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Request {
    bytes: Vec<u8>,
    call_id: String,
    resource_ref: Vec<u8>,
    kind: Kind,
    offset: u64,
    length: u32,
    path: String,
}

impl Request {
    pub fn decode(bytes: &[u8]) -> Result<Self> {
        let message = read(bytes)?;
        let root = message.get_root::<wire::message::Reader>().map_err(invalid)?;
        let wire::message::Request(request) = root.which().map_err(invalid)? else {
            return Err(invalid(()));
        };
        let request = request.map_err(invalid)?;
        if request.get_version() != VERSION
            || request.get_schema_digest().map_err(invalid)? != schema_digest()
        {
            return Err(Error::UnsupportedVersion);
        }
        let call_id = request.get_call_id().map_err(invalid)?;
        let call_id = call_id.to_str().map_err(invalid)?;
        identity(call_id)?;
        let resource_ref = request.get_resource_ref().map_err(invalid)?.to_vec();
        if resource_ref.len() != 32 {
            return Err(Error::Invalid("io resource ref"));
        }
        let kind = match request.get_kind().map_err(invalid)? {
            wire::Kind::FileRead => Kind::FileRead,
            wire::Kind::FileList => Kind::FileList,
            wire::Kind::FileCreate => Kind::FileCreate,
            wire::Kind::FileReplace => Kind::FileReplace,
            wire::Kind::FileDelete => Kind::FileDelete,
            wire::Kind::HttpGet => Kind::HttpGet,
            wire::Kind::HttpSend => Kind::HttpSend,
            wire::Kind::CredentialUse => Kind::CredentialUse,
        };
        let path = request.get_path().map_err(invalid)?;
        let path = path.to_str().map_err(invalid)?.to_owned();
        if path.len() > 1024 || path.chars().any(|c| c.is_control() || c == '\\' || c == '\0') {
            return Err(Error::Invalid("io path"));
        }
        if request.get_length() as usize > MAX_PAYLOAD_BYTES {
            return Err(Error::Limit);
        }
        Ok(Self {
            bytes: bytes.into(),
            call_id: call_id.into(),
            resource_ref,
            kind,
            offset: request.get_offset(),
            length: request.get_length(),
            path,
        })
    }

    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }
    pub fn kind(&self) -> Kind {
        self.kind
    }
    pub fn resource_ref(&self) -> &[u8] {
        &self.resource_ref
    }
    pub fn offset(&self) -> u64 {
        self.offset
    }
    pub fn length(&self) -> u32 {
        self.length
    }
    pub fn path(&self) -> &str {
        &self.path
    }

    /// W1 host reply: declared IO exists, no broker backend is attached.
    pub fn encode_unsupported(&self) -> Result<Vec<u8>> {
        let mut message = message();
        let root = message
            .get_root::<wire::message::Builder>()
            .map_err(invalid)?;
        let mut response = root.init_response();
        response.set_version(VERSION);
        response.set_schema_digest(&schema_digest());
        response.set_call_id(&self.call_id);
        response.set_request_sha256(&Sha256::digest(&self.bytes));
        response.set_status(wire::Status::Unsupported);
        response.set_eof(true);
        response.set_payload(b"");
        finish(&message)
    }
}
