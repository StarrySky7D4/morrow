//! Explicit qualification-only plan. It never supplies production authority.
use crate::{Result, wire};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::time::Instant;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PipeFaultPlan {
    pub version: u32,
    pub scenario: String,
    pub nonce: String,
    pub fixture_spec_sha256: String,
    pub target: String,
    pub prefix_bytes: usize,
    pub close_trigger: String,
}

#[cfg(test)]
#[path = "qualification_tests.rs"]
mod tests;
fn hex64(s: &str) -> bool {
    s.len() == 64
        && s.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
impl PipeFaultPlan {
    pub(crate) fn validate(&self, args: &[String]) -> Result<()> {
        if self.version != 1
            || self.scenario != "pipe-partial-close"
            || self.target != "first-response-body-chunk"
            || self.prefix_bytes != 12
            || self.close_trigger != "matched-passive-partial-frame-witness"
            || !hex64(&self.nonce)
            || self.nonce.bytes().all(|b| b == b'0')
            || !hex64(&self.fixture_spec_sha256)
        {
            return Err("qualification pipe plan bounds".into());
        }
        let matches: Vec<_> = args
            .iter()
            .enumerate()
            .filter(|(_, a)| a.as_str() == "--fixture-spec-sha256")
            .collect();
        if matches.len() != 1 || args.get(matches[0].0 + 1) != Some(&self.fixture_spec_sha256) {
            return Err("qualification plan is not bound to guest spec argument".into());
        }
        Ok(())
    }
    pub(crate) fn canonical(&self) -> Vec<u8> {
        serde_json::to_vec(self).expect("plan serialization")
    }
    pub(crate) fn digest(&self) -> String {
        wire::hex(&wire::digest(&self.canonical()))
    }
}
#[derive(Clone, Debug, Serialize)]
pub(crate) struct Identity {
    pub session: u64,
    pub epoch: u64,
    pub child_pid: u32,
    pub attempt: u64,
    pub operation_id_sha256: String,
    pub host_execution_config_sha256: String,
}
#[derive(Clone)]
pub(crate) struct BoundPlan {
    pub plan: PipeFaultPlan,
    pub identity: Identity,
    pub identity_sha256: String,
    pub deadline: Instant,
}
impl BoundPlan {
    pub(crate) fn new(
        plan: PipeFaultPlan,
        initial: &wire::Frame,
        deadline: Instant,
    ) -> Result<Self> {
        initial.validate().map_err(str::to_owned)?;
        if initial.kind != wire::Kind::Challenge
            || initial.revocation_generation != 1
            || Instant::now() >= deadline
        {
            return Err("qualification plan requires live original admission".into());
        }
        let identity = Identity {
            session: initial.session,
            epoch: initial.instance_epoch,
            child_pid: initial.child_pid,
            attempt: initial.attempt,
            operation_id_sha256: wire::hex(&wire::digest(&initial.operation_id)),
            host_execution_config_sha256: wire::hex(&initial.execution_config_sha256),
        };
        let identity_sha256 = wire::hex(&wire::digest(&serde_json::to_vec(&identity).unwrap()));
        Ok(Self {
            plan,
            identity,
            identity_sha256,
            deadline,
        })
    }
    pub(crate) fn description(&self) -> Value {
        json!({"plan":self.plan,"plan_sha256":self.plan.digest(),"identity":self.identity,
            "identity_sha256":self.identity_sha256,"generation":1,"deadline_renewed":false})
    }
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PartialFrameWitness {
    pub identity_sha256: String,
    pub nonce: String,
    pub fixture_spec_sha256: String,
    pub marker_sha256: String,
    pub marker_ordinal: u64,
    pub read_id: u64,
    pub read_issue_count: u64,
    pub buffered_bytes: usize,
    pub declared_payload_bytes: usize,
    pub expected_frame_bytes: usize,
    pub prefix_sha256: String,
}
#[derive(Clone)]
pub(crate) struct CutFrame {
    pub original: Vec<u8>,
    pub sequence: u64,
    pub body_offset: u64,
    pub body_end: u64,
    pub full_sha256: String,
    pub prefix_sha256: String,
}
impl CutFrame {
    pub(crate) fn new(bound: &BoundPlan, original: Vec<u8>, body_end: Option<u64>) -> Result<Self> {
        let frame = wire::Frame::decode(&original).map_err(str::to_owned)?;
        if frame.kind != wire::Kind::BodyChunk
            || frame.sequence != 2
            || frame.revocation_generation != 1
            || frame.session != bound.identity.session
            || frame.instance_epoch != bound.identity.epoch
            || frame.child_pid != bound.identity.child_pid
            || frame.attempt != bound.identity.attempt
            || wire::hex(&wire::digest(&frame.operation_id)) != bound.identity.operation_id_sha256
            || wire::hex(&frame.execution_config_sha256)
                != bound.identity.host_execution_config_sha256
            || original.len() <= 12
        {
            return Err("qualification target frame binding".into());
        }
        let wire::Payload::Chunk(chunk) = frame.payload else {
            return Err("qualification target is not BodyChunk".into());
        };
        let end = chunk
            .offset
            .checked_add(chunk.bytes.len() as u64)
            .ok_or("body end overflow")?;
        if chunk.offset != 0 || body_end != Some(end) {
            return Err("qualification first body range".into());
        }
        let declared = wire::payload_length(&original[..4]).map_err(str::to_owned)?;
        if declared + 4 != original.len() {
            return Err("qualification original length".into());
        }
        Ok(Self {
            sequence: frame.sequence,
            body_offset: chunk.offset,
            body_end: end,
            full_sha256: wire::hex(&wire::digest(&original)),
            prefix_sha256: wire::hex(&wire::digest(&original[..12])),
            original,
        })
    }
    pub(crate) fn description(&self) -> Value {
        json!({"sequence":self.sequence,"original_frame_sha256":self.full_sha256,
            "original_frame_raw_hex":wire::hex(&self.original),"raw_frame_fully_sent":false,
            "original_frame_bytes":self.original.len(),"declared_payload_bytes":self.original.len()-4,
            "body_offset":self.body_offset,"body_end":self.body_end,"prefix_sha256":self.prefix_sha256,
            "prefix_bytes":12,"original_frame_complete":false,"tail_reissued":false})
    }
    pub(crate) fn validate_witness(
        &self,
        bound: &BoundPlan,
        w: &PartialFrameWitness,
    ) -> Result<()> {
        if w.identity_sha256 != bound.identity_sha256
            || w.nonce != bound.plan.nonce
            || w.fixture_spec_sha256 != bound.plan.fixture_spec_sha256
            || !hex64(&w.marker_sha256)
            || w.marker_ordinal == 0
            || w.marker_ordinal > 128
            || w.read_id == 0
            || w.read_issue_count == 0
            || w.buffered_bytes != 12
            || w.expected_frame_bytes != self.original.len()
            || w.declared_payload_bytes != self.original.len() - 4
            || w.prefix_sha256 != self.prefix_sha256
        {
            return Err("qualification partial witness mismatch".into());
        }
        Ok(())
    }
}
