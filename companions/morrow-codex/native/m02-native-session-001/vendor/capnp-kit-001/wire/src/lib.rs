#![deny(unsafe_code)]
use capnp::{message, serialize};
use sha2::{Digest, Sha256};
#[allow(unsafe_code, clippy::all)]
pub mod native_session_capnp {
    include!(concat!(env!("OUT_DIR"), "/native_session_capnp.rs"));
}
pub use native_session_capnp::Kind;
pub const MAX_PAYLOAD: usize = 2048;
pub const MAX_FRAME: usize = MAX_PAYLOAD + 4;
pub const SCHEMA: &[u8] = include_bytes!("../native_session.capnp");
pub fn digest(bytes: &[u8]) -> [u8; 32] {
    Sha256::digest(bytes).into()
}
pub fn schema_digest() -> [u8; 32] {
    digest(SCHEMA)
}
pub fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
pub fn payload_length(prefix: &[u8]) -> Result<usize, &'static str> {
    if prefix.len() != 4 {
        return Err("length prefix");
    }
    let n = u32::from_le_bytes(prefix.try_into().unwrap()) as usize;
    if !(8..=MAX_PAYLOAD).contains(&n) || n % 8 != 0 {
        return Err("payload limit/alignment");
    }
    Ok(n)
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Frame {
    pub kind: Kind,
    pub sequence: u64,
    pub session: u64,
    pub epoch: u64,
    pub generation: u64,
    pub pid: u32,
    pub code: u32,
    pub remaining_ms: u64,
    pub nonce: [u8; 32],
    pub schema: [u8; 32],
    pub artifact: [u8; 32],
    pub config: [u8; 32],
    pub budget: u64,
    pub capabilities: u64,
}
impl Frame {
    pub fn request(&self, kind: Kind, sequence: u64) -> Self {
        let mut f = self.clone();
        f.kind = kind;
        f.sequence = sequence;
        f.code = 0;
        f
    }
    pub fn encode(&self) -> Vec<u8> {
        let mut msg = message::Builder::new_default();
        {
            let mut f = msg.init_root::<native_session_capnp::native_session::Builder>();
            f.set_major(2);
            f.set_revision(1);
            f.set_kind(self.kind);
            f.set_sequence(self.sequence);
            f.set_session(self.session);
            f.set_instance_epoch(self.epoch);
            f.set_revocation_generation(self.generation);
            f.set_child_pid(self.pid);
            f.set_code(self.code);
            f.set_remaining_ms(self.remaining_ms);
            f.set_nonce(&self.nonce);
            f.set_schema_sha256(&self.schema);
            f.set_artifact_sha256(&self.artifact);
            f.set_execution_config_sha256(&self.config);
            f.set_request_budget(self.budget);
            f.set_capabilities(self.capabilities);
            f.set_reserved(0);
        }
        let bytes = serialize::write_message_to_words(&msg);
        assert!(bytes.len() <= MAX_PAYLOAD);
        let mut out = (bytes.len() as u32).to_le_bytes().to_vec();
        out.extend(bytes);
        out
    }
    pub fn decode(bytes: &[u8]) -> Result<Self, &'static str> {
        if bytes.len() < 4 {
            return Err("short frame");
        }
        let n = payload_length(&bytes[..4])?;
        if bytes.len() != n + 4 {
            return Err("frame length");
        }
        let mut cursor = std::io::Cursor::new(&bytes[4..]);
        let mut opts = message::ReaderOptions::new();
        opts.traversal_limit_in_words(Some(1024));
        opts.nesting_limit(8);
        let msg = serialize::read_message(&mut cursor, opts).map_err(|_| "Capnp decode")?;
        if cursor.position() as usize != n {
            return Err("trailing message");
        }
        let f = msg
            .get_root::<native_session_capnp::native_session::Reader>()
            .map_err(|_| "root")?;
        if f.get_major() != 2 || f.get_revision() != 1 || f.get_reserved() != 0 {
            return Err("version/reserved");
        }
        let data = |v: capnp::Result<&[u8]>| -> Result<[u8; 32], &'static str> {
            v.map_err(|_| "data pointer")?
                .try_into()
                .map_err(|_| "digest length")
        };
        Ok(Self {
            kind: f.get_kind().map_err(|_| "kind")?,
            sequence: f.get_sequence(),
            session: f.get_session(),
            epoch: f.get_instance_epoch(),
            generation: f.get_revocation_generation(),
            pid: f.get_child_pid(),
            code: f.get_code(),
            remaining_ms: f.get_remaining_ms(),
            nonce: data(f.get_nonce())?,
            schema: data(f.get_schema_sha256())?,
            artifact: data(f.get_artifact_sha256())?,
            config: data(f.get_execution_config_sha256())?,
            budget: f.get_request_budget(),
            capabilities: f.get_capabilities(),
        })
    }
    /// Comparison only: caller MUST separately validate direction/kind/sequence/code.
    pub fn matches_admission(&self, initial: &Self) -> bool {
        self.request(initial.kind, initial.sequence)
            == initial.request(initial.kind, initial.sequence)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn roundtrip_bounds_and_identity() {
        let f = Frame {
            kind: Kind::Challenge,
            sequence: 0,
            session: 1,
            epoch: 2,
            generation: 1,
            pid: 3,
            code: 0,
            remaining_ms: 1000,
            nonce: [4; 32],
            schema: schema_digest(),
            artifact: [5; 32],
            config: [6; 32],
            budget: 16,
            capabilities: 1,
        };
        let b = f.encode();
        assert_eq!(Frame::decode(&b).unwrap(), f);
        for n in 0..b.len() {
            assert!(Frame::decode(&b[..n]).is_err());
        }
        assert!(payload_length(&4096u32.to_le_bytes()).is_err());
        let mut bad = b.clone();
        bad.extend([0; 8]);
        assert!(Frame::decode(&bad).is_err());
        let mut r = f.request(Kind::Hello, 1);
        assert!(r.matches_admission(&f));
        r.epoch += 1;
        assert!(!r.matches_admission(&f));
    }
}
