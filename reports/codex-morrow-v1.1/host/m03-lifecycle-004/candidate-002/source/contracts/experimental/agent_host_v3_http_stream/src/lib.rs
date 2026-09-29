#![deny(unsafe_code)]
//! Owned bounded codec. Decoding never grants authority or advances an attempt.
mod rules;
use capnp::{message, serialize};
pub use rules::request_digest;
use sha2::{Digest, Sha256};
#[allow(unsafe_code, clippy::all)]
pub mod native_http_capnp {
    include!(concat!(env!("OUT_DIR"), "/native_http_capnp.rs"));
}
pub use native_http_capnp::{IntentPhase, Kind, NetworkPhase, OwnerPhase};
pub type Result<T> = std::result::Result<T, &'static str>;
pub const MAX_PAYLOAD: usize = 32768;
pub const MAX_FRAME: usize = MAX_PAYLOAD + 4;
pub const MAX_BODY: usize = 32768;
pub const MAX_RESPONSE: usize = 65536;
pub const MAX_CHUNK: usize = 8192;
pub const MAX_CREDIT: u32 = 16384;
pub const SCHEMA: &[u8] = include_bytes!("../native_http.capnp");
pub fn digest(bytes: &[u8]) -> [u8; 32] {
    Sha256::digest(bytes).into()
}
pub fn schema_digest() -> [u8; 32] {
    digest(SCHEMA)
}
pub fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
pub fn payload_length(prefix: &[u8]) -> Result<usize> {
    let p: [u8; 4] = prefix.try_into().map_err(|_| "prefix")?;
    let n = u32::from_le_bytes(p) as usize;
    if !(8..=MAX_PAYLOAD).contains(&n) || n % 8 != 0 {
        return Err("payload limit/alignment");
    }
    Ok(n)
}
#[derive(Clone, Debug, PartialEq)]
pub struct Channel {
    pub locator: String,
    pub nonce: Vec<u8>,
    pub max_chunk_bytes: u32,
    pub credit_limit: u32,
}
impl Channel {
    fn write(&self, mut b: native_http_capnp::channel::Builder<'_>) {
        b.set_locator(&self.locator);
        b.set_nonce(&self.nonce);
        b.set_max_chunk_bytes(self.max_chunk_bytes);
        b.set_credit_limit(self.credit_limit);
    }
    fn read(r: native_http_capnp::channel::Reader<'_>) -> Result<Self> {
        Ok(Self {
            locator: r
                .get_locator()
                .map_err(|_| "text pointer")?
                .to_str()
                .map_err(|_| "UTF8")?
                .to_owned(),
            nonce: r.get_nonce().map_err(|_| "data pointer")?.to_vec(),
            max_chunk_bytes: r.get_max_chunk_bytes(),
            credit_limit: r.get_credit_limit(),
        })
    }
}
#[derive(Clone, Debug, PartialEq)]
pub struct Header {
    pub name: String,
    pub value: Vec<u8>,
}
impl Header {
    fn write(&self, mut b: native_http_capnp::header::Builder<'_>) {
        b.set_name(&self.name);
        b.set_value(&self.value);
    }
    fn read(r: native_http_capnp::header::Reader<'_>) -> Result<Self> {
        Ok(Self {
            name: r
                .get_name()
                .map_err(|_| "text pointer")?
                .to_str()
                .map_err(|_| "UTF8")?
                .to_owned(),
            value: r.get_value().map_err(|_| "data pointer")?.to_vec(),
        })
    }
}
#[derive(Clone, Debug, PartialEq)]
pub struct Prepare {
    pub method: String,
    pub absolute_target: String,
    pub headers: Vec<Header>,
    pub body_bytes: u32,
    pub body_sha256: Vec<u8>,
    pub response_limit_bytes: u32,
}
impl Prepare {
    fn write(&self, mut b: native_http_capnp::prepare::Builder<'_>) {
        b.set_method(&self.method);
        b.set_absolute_target(&self.absolute_target);
        {
            let mut list = b.reborrow().init_headers(self.headers.len() as u32);
            for (i, value) in self.headers.iter().enumerate() {
                value.write(list.reborrow().get(i as u32));
            }
        }
        b.set_body_bytes(self.body_bytes);
        b.set_body_sha256(&self.body_sha256);
        b.set_response_limit_bytes(self.response_limit_bytes);
    }
    fn read(r: native_http_capnp::prepare::Reader<'_>) -> Result<Self> {
        Ok(Self {
            method: r
                .get_method()
                .map_err(|_| "text pointer")?
                .to_str()
                .map_err(|_| "UTF8")?
                .to_owned(),
            absolute_target: r
                .get_absolute_target()
                .map_err(|_| "text pointer")?
                .to_str()
                .map_err(|_| "UTF8")?
                .to_owned(),
            headers: {
                let list = r.get_headers().map_err(|_| "headers")?;
                if list.len() > 32 {
                    return Err("header count");
                }
                list.iter().map(Header::read).collect::<Result<Vec<_>>>()?
            },
            body_bytes: r.get_body_bytes(),
            body_sha256: r.get_body_sha256().map_err(|_| "data pointer")?.to_vec(),
            response_limit_bytes: r.get_response_limit_bytes(),
        })
    }
}
#[derive(Clone, Debug, PartialEq)]
pub struct Chunk {
    pub offset: u64,
    pub bytes: Vec<u8>,
}
impl Chunk {
    fn write(&self, mut b: native_http_capnp::chunk::Builder<'_>) {
        b.set_offset(self.offset);
        b.set_bytes(&self.bytes);
    }
    fn read(r: native_http_capnp::chunk::Reader<'_>) -> Result<Self> {
        Ok(Self {
            offset: r.get_offset(),
            bytes: r.get_bytes().map_err(|_| "data pointer")?.to_vec(),
        })
    }
}
#[derive(Clone, Debug, PartialEq)]
pub struct Decision {
    pub proposal_ref: Vec<u8>,
    pub body_sha256: Vec<u8>,
    pub request_sha256: Vec<u8>,
    pub http_grant_ref: Vec<u8>,
    pub endpoint_ref: Vec<u8>,
    pub body_bytes: u32,
    pub send_budget: u32,
    pub response_limit_bytes: u32,
}
impl Decision {
    fn write(&self, mut b: native_http_capnp::decision::Builder<'_>) {
        b.set_proposal_ref(&self.proposal_ref);
        b.set_body_sha256(&self.body_sha256);
        b.set_request_sha256(&self.request_sha256);
        b.set_http_grant_ref(&self.http_grant_ref);
        b.set_endpoint_ref(&self.endpoint_ref);
        b.set_body_bytes(self.body_bytes);
        b.set_send_budget(self.send_budget);
        b.set_response_limit_bytes(self.response_limit_bytes);
    }
    fn read(r: native_http_capnp::decision::Reader<'_>) -> Result<Self> {
        Ok(Self {
            proposal_ref: r.get_proposal_ref().map_err(|_| "data pointer")?.to_vec(),
            body_sha256: r.get_body_sha256().map_err(|_| "data pointer")?.to_vec(),
            request_sha256: r.get_request_sha256().map_err(|_| "data pointer")?.to_vec(),
            http_grant_ref: r.get_http_grant_ref().map_err(|_| "data pointer")?.to_vec(),
            endpoint_ref: r.get_endpoint_ref().map_err(|_| "data pointer")?.to_vec(),
            body_bytes: r.get_body_bytes(),
            send_budget: r.get_send_budget(),
            response_limit_bytes: r.get_response_limit_bytes(),
        })
    }
}
#[derive(Clone, Debug, PartialEq)]
pub struct Head {
    pub status: u16,
    pub headers: Vec<Header>,
    pub remote_address: String,
}
impl Head {
    fn write(&self, mut b: native_http_capnp::head::Builder<'_>) {
        b.set_status(self.status);
        {
            let mut list = b.reborrow().init_headers(self.headers.len() as u32);
            for (i, value) in self.headers.iter().enumerate() {
                value.write(list.reborrow().get(i as u32));
            }
        }
        b.set_remote_address(&self.remote_address);
    }
    fn read(r: native_http_capnp::head::Reader<'_>) -> Result<Self> {
        Ok(Self {
            status: r.get_status(),
            headers: {
                let list = r.get_headers().map_err(|_| "headers")?;
                if list.len() > 32 {
                    return Err("header count");
                }
                list.iter().map(Header::read).collect::<Result<Vec<_>>>()?
            },
            remote_address: r
                .get_remote_address()
                .map_err(|_| "text pointer")?
                .to_str()
                .map_err(|_| "UTF8")?
                .to_owned(),
        })
    }
}
#[derive(Clone, Debug, PartialEq)]
pub struct Credit {
    pub consumed_offset: u64,
    pub parser_yielded_bytes: u64,
    pub drain_discarded_bytes: u64,
    pub cancel_discarded_bytes: u64,
    pub window_bytes: u32,
    pub max_chunk_bytes: u32,
    pub error_consumed_bytes: u64,
}
impl Credit {
    fn write(&self, mut b: native_http_capnp::credit::Builder<'_>) {
        b.set_consumed_offset(self.consumed_offset);
        b.set_parser_yielded_bytes(self.parser_yielded_bytes);
        b.set_drain_discarded_bytes(self.drain_discarded_bytes);
        b.set_cancel_discarded_bytes(self.cancel_discarded_bytes);
        b.set_window_bytes(self.window_bytes);
        b.set_max_chunk_bytes(self.max_chunk_bytes);
        b.set_error_consumed_bytes(self.error_consumed_bytes);
    }
    fn read(r: native_http_capnp::credit::Reader<'_>) -> Result<Self> {
        Ok(Self {
            consumed_offset: r.get_consumed_offset(),
            parser_yielded_bytes: r.get_parser_yielded_bytes(),
            drain_discarded_bytes: r.get_drain_discarded_bytes(),
            cancel_discarded_bytes: r.get_cancel_discarded_bytes(),
            window_bytes: r.get_window_bytes(),
            max_chunk_bytes: r.get_max_chunk_bytes(),
            error_consumed_bytes: r.get_error_consumed_bytes(),
        })
    }
}
#[derive(Clone, Debug, PartialEq)]
pub struct Progress {
    pub intent: IntentPhase,
    pub network: NetworkPhase,
    pub owner: OwnerPhase,
    pub http_status: u16,
    pub error_code: u32,
    pub received_offset: u64,
    pub reserved_offset: u64,
    pub issued_offset: u64,
    pub os_completed_offset: u64,
    pub peer_consumed_offset: u64,
    pub parser_yielded_bytes: u64,
    pub drain_discarded_bytes: u64,
    pub cancel_discarded_bytes: u64,
    pub last_write_ordinal: u64,
    pub revoke_persisted: bool,
    pub revoke_applied: bool,
    pub http_eof: bool,
    pub response_material_stored: bool,
    pub worker_joined: bool,
    pub connect_reaped: bool,
    pub read_reaped: bool,
    pub write_reaped: bool,
    pub data_closed: bool,
    pub child_exited: bool,
    pub stdout_eof: bool,
    pub stderr_eof: bool,
    pub owner_released: bool,
    pub worker_started: bool,
    pub error_consumed_bytes: u64,
    pub request_closed: bool,
}
impl Progress {
    fn write(&self, mut b: native_http_capnp::progress::Builder<'_>) {
        b.set_intent(self.intent);
        b.set_network(self.network);
        b.set_owner(self.owner);
        b.set_http_status(self.http_status);
        b.set_error_code(self.error_code);
        b.set_received_offset(self.received_offset);
        b.set_reserved_offset(self.reserved_offset);
        b.set_issued_offset(self.issued_offset);
        b.set_os_completed_offset(self.os_completed_offset);
        b.set_peer_consumed_offset(self.peer_consumed_offset);
        b.set_parser_yielded_bytes(self.parser_yielded_bytes);
        b.set_drain_discarded_bytes(self.drain_discarded_bytes);
        b.set_cancel_discarded_bytes(self.cancel_discarded_bytes);
        b.set_last_write_ordinal(self.last_write_ordinal);
        b.set_revoke_persisted(self.revoke_persisted);
        b.set_revoke_applied(self.revoke_applied);
        b.set_http_eof(self.http_eof);
        b.set_response_material_stored(self.response_material_stored);
        b.set_worker_joined(self.worker_joined);
        b.set_connect_reaped(self.connect_reaped);
        b.set_read_reaped(self.read_reaped);
        b.set_write_reaped(self.write_reaped);
        b.set_data_closed(self.data_closed);
        b.set_child_exited(self.child_exited);
        b.set_stdout_eof(self.stdout_eof);
        b.set_stderr_eof(self.stderr_eof);
        b.set_owner_released(self.owner_released);
        b.set_worker_started(self.worker_started);
        b.set_error_consumed_bytes(self.error_consumed_bytes);
        b.set_request_closed(self.request_closed);
    }
    fn read(r: native_http_capnp::progress::Reader<'_>) -> Result<Self> {
        Ok(Self {
            intent: r.get_intent().map_err(|_| "enum")?,
            network: r.get_network().map_err(|_| "enum")?,
            owner: r.get_owner().map_err(|_| "enum")?,
            http_status: r.get_http_status(),
            error_code: r.get_error_code(),
            received_offset: r.get_received_offset(),
            reserved_offset: r.get_reserved_offset(),
            issued_offset: r.get_issued_offset(),
            os_completed_offset: r.get_os_completed_offset(),
            peer_consumed_offset: r.get_peer_consumed_offset(),
            parser_yielded_bytes: r.get_parser_yielded_bytes(),
            drain_discarded_bytes: r.get_drain_discarded_bytes(),
            cancel_discarded_bytes: r.get_cancel_discarded_bytes(),
            last_write_ordinal: r.get_last_write_ordinal(),
            revoke_persisted: r.get_revoke_persisted(),
            revoke_applied: r.get_revoke_applied(),
            http_eof: r.get_http_eof(),
            response_material_stored: r.get_response_material_stored(),
            worker_joined: r.get_worker_joined(),
            connect_reaped: r.get_connect_reaped(),
            read_reaped: r.get_read_reaped(),
            write_reaped: r.get_write_reaped(),
            data_closed: r.get_data_closed(),
            child_exited: r.get_child_exited(),
            stdout_eof: r.get_stdout_eof(),
            stderr_eof: r.get_stderr_eof(),
            owner_released: r.get_owner_released(),
            worker_started: r.get_worker_started(),
            error_consumed_bytes: r.get_error_consumed_bytes(),
            request_closed: r.get_request_closed(),
        })
    }
}
#[derive(Clone, Debug, PartialEq)]
pub enum Payload {
    None,
    Channel(Channel),
    Prepare(Prepare),
    Chunk(Chunk),
    Decision(Decision),
    Head(Head),
    Credit(Credit),
    Progress(Progress),
}
#[derive(Clone, Debug, PartialEq)]
pub struct Frame {
    pub kind: Kind,
    pub sequence: u64,
    pub session: u64,
    pub instance_epoch: u64,
    pub revocation_generation: u64,
    pub child_pid: u32,
    pub code: u32,
    pub remaining_ms: u64,
    pub nonce: Vec<u8>,
    pub schema_sha256: Vec<u8>,
    pub artifact_sha256: Vec<u8>,
    pub execution_config_sha256: Vec<u8>,
    pub request_budget: u64,
    pub capabilities: u64,
    pub operation_id: Vec<u8>,
    pub attempt: u64,
    pub payload: Payload,
}
impl Frame {
    pub fn encode(&self) -> Result<Vec<u8>> {
        self.validate()?;
        let mut msg = message::Builder::new_default();
        {
            let mut b = msg.init_root::<native_http_capnp::frame::Builder>();
            b.set_major(3);
            b.set_revision(1);
            b.set_reserved(0);
            b.set_kind(self.kind);
            b.set_sequence(self.sequence);
            b.set_session(self.session);
            b.set_instance_epoch(self.instance_epoch);
            b.set_revocation_generation(self.revocation_generation);
            b.set_child_pid(self.child_pid);
            b.set_code(self.code);
            b.set_remaining_ms(self.remaining_ms);
            b.set_nonce(&self.nonce);
            b.set_schema_sha256(&self.schema_sha256);
            b.set_artifact_sha256(&self.artifact_sha256);
            b.set_execution_config_sha256(&self.execution_config_sha256);
            b.set_request_budget(self.request_budget);
            b.set_capabilities(self.capabilities);
            b.set_operation_id(&self.operation_id);
            b.set_attempt(self.attempt);

            let mut p = b.init_payload();
            match &self.payload {
                Payload::None => p.set_none(()),
                Payload::Channel(value) => value.write(p.init_channel()),
                Payload::Prepare(value) => value.write(p.init_prepare()),
                Payload::Chunk(value) => value.write(p.init_chunk()),
                Payload::Decision(value) => value.write(p.init_decision()),
                Payload::Head(value) => value.write(p.init_head()),
                Payload::Credit(value) => value.write(p.init_credit()),
                Payload::Progress(value) => value.write(p.init_progress()),
            }
        }
        let bytes = serialize::write_message_to_words(&msg);
        if bytes.len() > MAX_PAYLOAD {
            return Err("encoded limit");
        }
        let mut frame = (bytes.len() as u32).to_le_bytes().to_vec();
        frame.extend(bytes);
        Ok(frame)
    }
    pub fn decode(bytes: &[u8]) -> Result<Self> {
        if bytes.len() < 4 {
            return Err("short");
        }
        let n = payload_length(&bytes[..4])?;
        if bytes.len() != n + 4 {
            return Err("frame length");
        }
        let mut cursor = std::io::Cursor::new(&bytes[4..]);
        let mut opts = message::ReaderOptions::new();
        opts.traversal_limit_in_words(Some(16384));
        opts.nesting_limit(8);
        let msg = serialize::read_message(&mut cursor, opts).map_err(|_| "Capnp decode")?;
        if cursor.position() as usize != n {
            return Err("trailing message");
        }
        let r = msg
            .get_root::<native_http_capnp::frame::Reader>()
            .map_err(|_| "root")?;
        if r.get_major() != 3 || r.get_revision() != 1 || r.get_reserved() != 0 {
            return Err("version/reserved");
        }
        use native_http_capnp::frame::payload::Which;
        let payload = match r.get_payload().which().map_err(|_| "union")? {
            Which::None(()) => Payload::None,
            Which::Channel(value) => {
                Payload::Channel(Channel::read(value.map_err(|_| "payload pointer")?)?)
            }
            Which::Prepare(value) => {
                Payload::Prepare(Prepare::read(value.map_err(|_| "payload pointer")?)?)
            }
            Which::Chunk(value) => {
                Payload::Chunk(Chunk::read(value.map_err(|_| "payload pointer")?)?)
            }
            Which::Decision(value) => {
                Payload::Decision(Decision::read(value.map_err(|_| "payload pointer")?)?)
            }
            Which::Head(value) => Payload::Head(Head::read(value.map_err(|_| "payload pointer")?)?),
            Which::Credit(value) => {
                Payload::Credit(Credit::read(value.map_err(|_| "payload pointer")?)?)
            }
            Which::Progress(value) => {
                Payload::Progress(Progress::read(value.map_err(|_| "payload pointer")?)?)
            }
        };
        let value = Self {
            kind: r.get_kind().map_err(|_| "enum")?,
            sequence: r.get_sequence(),
            session: r.get_session(),
            instance_epoch: r.get_instance_epoch(),
            revocation_generation: r.get_revocation_generation(),
            child_pid: r.get_child_pid(),
            code: r.get_code(),
            remaining_ms: r.get_remaining_ms(),
            nonce: r.get_nonce().map_err(|_| "data pointer")?.to_vec(),
            schema_sha256: r.get_schema_sha256().map_err(|_| "data pointer")?.to_vec(),
            artifact_sha256: r
                .get_artifact_sha256()
                .map_err(|_| "data pointer")?
                .to_vec(),
            execution_config_sha256: r
                .get_execution_config_sha256()
                .map_err(|_| "data pointer")?
                .to_vec(),
            request_budget: r.get_request_budget(),
            capabilities: r.get_capabilities(),
            operation_id: r.get_operation_id().map_err(|_| "data pointer")?.to_vec(),
            attempt: r.get_attempt(),
            payload,
        };
        value.validate()?;
        Ok(value)
    }
}
