//! Host-only selected-file broker. Guests cannot construct grants or open OS paths.
use crate::{
    Error, Result,
    io::{self, Kind, Request, Response, Status},
};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};

pub const MAX_RESOURCES: usize = 8;
pub const MAX_SELECTED_BYTES: usize = 16 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IoIdentity {
    pub host: u64,
    pub connection: u64,
    pub generation: u64,
    pub package_digest: [u8; 32],
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApprovedIo {
    kinds: BTreeSet<Kind>,
}

impl ApprovedIo {
    /// Trusted host/manager projection. Callers must not invent this from guest bytes.
    pub fn from_manager(kinds: BTreeSet<Kind>) -> Self {
        Self { kinds }
    }
    pub fn allows(&self, kind: Kind) -> bool {
        self.kinds.contains(&kind)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SelectedFile {
    bytes: Vec<u8>,
}

impl SelectedFile {
    /// Host copies the chosen file into a private spool. The original path is discarded.
    pub fn from_fixed_bytes(bytes: Vec<u8>) -> Result<Self> {
        if bytes.len() > MAX_SELECTED_BYTES {
            return Err(Error::Limit);
        }
        Ok(Self { bytes })
    }
    pub fn len(&self) -> usize {
        self.bytes.len()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResourceRef {
    token: [u8; 32],
}

impl ResourceRef {
    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.token
    }
}

struct Lease {
    identity: IoIdentity,
    kind: Kind,
    expires: u64,
    spool: Vec<u8>,
}

pub struct IoBroker {
    secret: [u8; 32],
    seq: u64,
    leases: BTreeMap<[u8; 32], Lease>,
}

impl IoBroker {
    pub fn new(secret: [u8; 32]) -> Self {
        Self {
            secret,
            seq: 0,
            leases: BTreeMap::new(),
        }
    }

    pub fn grant_file(
        &mut self,
        identity: IoIdentity,
        approval: &ApprovedIo,
        selected: SelectedFile,
        expires: u64,
        now: u64,
    ) -> Result<ResourceRef> {
        if now >= expires {
            return Err(Error::Invalid("expired grant"));
        }
        if !approval.allows(Kind::FileRead) {
            return Err(Error::Invalid("io denied"));
        }
        if self.leases.len() >= MAX_RESOURCES {
            return Err(Error::Limit);
        }
        self.seq = self.seq.checked_add(1).ok_or(Error::Limit)?;
        let mut digest = Sha256::new();
        digest.update(self.secret);
        digest.update(self.seq.to_le_bytes());
        digest.update(identity.host.to_le_bytes());
        digest.update(identity.connection.to_le_bytes());
        digest.update(identity.generation.to_le_bytes());
        digest.update(identity.package_digest);
        let token: [u8; 32] = digest.finalize().into();
        self.leases.insert(
            token,
            Lease {
                identity,
                kind: Kind::FileRead,
                expires,
                spool: selected.bytes,
            },
        );
        Ok(ResourceRef { token })
    }

    pub fn revoke(&mut self, identity: &IoIdentity) {
        self.leases.retain(|_, lease| {
            !(lease.identity.host == identity.host
                && lease.identity.connection == identity.connection)
        });
    }

    pub fn exchange(
        &mut self,
        identity: &IoIdentity,
        approval: &ApprovedIo,
        request: &[u8],
        now: u64,
        cancelled: bool,
    ) -> Result<Vec<u8>> {
        let request = Request::decode(request)?;
        if cancelled {
            return request.encode_status(Status::Cancelled, true, b"");
        }
        if !approval.allows(request.kind()) {
            return request.encode_status(Status::Denied, true, b"");
        }
        if request.kind() != Kind::FileRead {
            return request.encode_status(Status::Unsupported, true, b"");
        }
        if !request.path().is_empty() {
            return request.encode_status(Status::InvalidPath, true, b"");
        }
        let token: [u8; 32] = request
            .resource_ref()
            .try_into()
            .map_err(|_| Error::Invalid("io resource ref"))?;
        let Some(lease) = self.leases.get(&token) else {
            return request.encode_status(Status::Revoked, true, b"");
        };
        if lease.identity != *identity {
            return request.encode_status(Status::Denied, true, b"");
        }
        if now >= lease.expires {
            return request.encode_status(Status::Expired, true, b"");
        }
        if lease.kind != request.kind() {
            return request.encode_status(Status::Denied, true, b"");
        }
        let start = request.offset();
        if start > lease.spool.len() as u64 {
            return request.encode_status(Status::Quota, true, b"");
        }
        let start = start as usize;
        let want = if request.length() == 0 {
            io::MAX_CHUNK_BYTES as usize
        } else {
            request.length() as usize
        };
        let want = want.min(io::MAX_CHUNK_BYTES as usize);
        let end = (start + want).min(lease.spool.len());
        let chunk = lease.spool[start..end].to_vec();
        let eof = end == lease.spool.len();
        request.encode_status(Status::Ok, eof, &chunk)
    }
}

impl Response {
    pub fn decode_for(request: &Request, bytes: &[u8]) -> Result<Self> {
        Response::decode(request, bytes)
    }
}
