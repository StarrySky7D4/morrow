use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

/// Compatibility identity, never a substitute for the retained executable pin.
pub const CHECKED_CONTROL_SCHEMA: [u8; 32] = [
    47, 129, 224, 48, 226, 133, 48, 72, 12, 233, 197, 28, 212, 105, 181, 9, 123, 169, 250, 219, 59,
    123, 116, 198, 60, 193, 216, 59, 17, 20, 235, 24,
];
pub const MAX_CONTROL_IDS: usize = 256;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ControlHello {
    pub nonce: [u8; 32],
    pub schema: [u8; 32],
}
impl ControlHello {
    pub fn valid(&self) -> bool {
        self.nonce != [0; 32] && self.schema == CHECKED_CONTROL_SCHEMA
    }
}

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct RunnerControlCapabilities {
    pub close_stdin: bool,
    pub resize: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ControlHelloAck {
    pub hello: ControlHello,
    pub controls: RunnerControlCapabilities,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ControlOperation {
    CloseStdin,
    Resize,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ControlRequest {
    pub id: u64,
    pub nonce: [u8; 32],
    pub op: ControlOperation,
    pub rows: u16,
    pub cols: u16,
    pub args_digest: [u8; 32],
}
impl ControlRequest {
    pub fn new(id: u64, nonce: [u8; 32], op: ControlOperation, rows: u16, cols: u16) -> Self {
        Self {
            id,
            nonce,
            op,
            rows,
            cols,
            args_digest: Self::digest(op, rows, cols),
        }
    }
    fn digest(op: ControlOperation, rows: u16, cols: u16) -> [u8; 32] {
        let mut digest = Sha256::new();
        digest.update(b"morrow.c17.runner.control.args.v1\0");
        digest.update([match op {
            ControlOperation::CloseStdin => 1,
            ControlOperation::Resize => 2,
        }]);
        digest.update(rows.to_le_bytes());
        digest.update(cols.to_le_bytes());
        digest.finalize().into()
    }
    pub fn valid(&self, nonce: [u8; 32]) -> bool {
        self.id != 0
            && self.nonce == nonce
            && self.args_digest == Self::digest(self.op, self.rows, self.cols)
            && match self.op {
                ControlOperation::CloseStdin => self.rows == 0 && self.cols == 0,
                ControlOperation::Resize => {
                    (1..=4096).contains(&self.rows) && (1..=4096).contains(&self.cols)
                }
            }
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ControlStatus {
    Applied,
    Rejected,
    Unsupported,
    Unknown,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ControlResult {
    pub request: ControlRequest,
    pub status: ControlStatus,
    pub os_error: Option<i32>,
}
impl ControlResult {
    pub fn new(request: ControlRequest, status: ControlStatus, os_error: Option<i32>) -> Self {
        Self {
            request,
            status,
            os_error,
        }
    }
}

pub enum ControlAdmission {
    Apply,
    Observe(ControlResult),
}

/// Runner-owned bounded tombstones. Only the single input actor calls this.
pub struct ControlLedger {
    nonce: [u8; 32],
    last_id: u64,
    results: BTreeMap<u64, ControlResult>,
}
impl ControlLedger {
    pub fn new(nonce: [u8; 32]) -> Self {
        Self {
            nonce,
            last_id: 0,
            results: BTreeMap::new(),
        }
    }
    pub fn admit(&mut self, request: &ControlRequest) -> ControlAdmission {
        if let Some(prior) = self.results.get(&request.id) {
            return ControlAdmission::Observe(if prior.request == *request {
                prior.clone()
            } else {
                ControlResult::new(request.clone(), ControlStatus::Rejected, None)
            });
        }
        if !request.valid(self.nonce)
            || request.id <= self.last_id
            || self.results.len() >= MAX_CONTROL_IDS
        {
            return ControlAdmission::Observe(ControlResult::new(
                request.clone(),
                ControlStatus::Rejected,
                None,
            ));
        }
        self.last_id = request.id;
        // The unknown tombstone precedes the real OS operation. Never erase it.
        self.results.insert(
            request.id,
            ControlResult::new(request.clone(), ControlStatus::Unknown, None),
        );
        ControlAdmission::Apply
    }
    pub fn finish(&mut self, result: ControlResult) -> bool {
        let Some(prior) = self.results.get_mut(&result.request.id) else {
            return false;
        };
        if prior.request != result.request || prior.status != ControlStatus::Unknown {
            return false;
        }
        *prior = result;
        true
    }
}

#[cfg(test)]
#[path = "checked_protocol_tests.rs"]
mod tests;
