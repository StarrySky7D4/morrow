//! Precompiled internal supervision protocol. Decoding never grants authority.
use crate::{Result, supervision_record_codec as model, workbench_supervision_capnp as wire};
use model::proto::{BusinessOutcome, ObservedTruth, OwnerPhase, ProtocolAck};
use model::{Identity, OwnerRecord, ReaderJoins, ResourceFacts};

pub const WIRE_VERSION: u16 = 1;
pub const MAX_PACKET: usize = 65_536;
pub const SCHEMA_SHA256: &[u8; 32] =
    include_bytes!(concat!(env!("OUT_DIR"), "/supervision-schema.sha256"));

#[derive(Clone, Debug)]
pub struct PreparedAbnormal {
    pub identity: Identity,
    pub committed_digest: [u8; 32],
    pub resources: ResourceFacts,
    pub reader_joins: ReaderJoins,
}

#[derive(Clone, Debug)]
pub struct Status {
    pub record: OwnerRecord,
    pub phase: wire::StatusPhase,
    pub disposition: wire::OwnerDisposition,
    pub durable_proof: bool,
    pub abnormal_protocol: u16,
    pub prepared: Option<PreparedAbnormal>,
}

#[derive(Clone, Debug)]
pub struct Preview {
    pub profile: [u8; 32],
    pub token: [u8; 32],
    pub original_digest: [u8; 32],
    pub history_head: Vec<u8>,
    pub eligible: bool,
    pub reason: String,
    pub resource_recovery_confirmed: bool,
    pub recovery_original_digest: Option<[u8; 32]>,
    pub record: OwnerRecord,
}

#[derive(Clone, Debug)]
pub struct Recovery {
    pub profile: [u8; 32],
    pub request_token: [u8; 32],
    pub original_digest: [u8; 32],
    pub history_head: [u8; 32],
    pub record: OwnerRecord,
}

pub enum Message<'a> {
    Status(&'a Status),
    Preview(&'a Preview),
    Recovery(&'a Recovery),
    Failure { reason: &'a str, detail: &'a str },
}

impl Status {
    pub fn validate(&self) -> Result<()> {
        model::validate(&self.record)?;
        if model::identity(&self.record)?.child_pid == 0
            || model::identity(&self.record)?.child_creation_filetime == 0
            || ![0, 1].contains(&self.abnormal_protocol)
        {
            return Err("Invalid supervision status identity or extension".into());
        }
        let owner = OwnerPhase::try_from(self.record.phase)?;
        let valid = match self.phase {
            wire::StatusPhase::Running => {
                owner == OwnerPhase::Active && self.disposition == wire::OwnerDisposition::Active
            }
            wire::StatusPhase::Closing => {
                owner == OwnerPhase::Closing && self.disposition == wire::OwnerDisposition::Closing
            }
            wire::StatusPhase::Released => {
                owner == OwnerPhase::Released
                    && self.disposition == wire::OwnerDisposition::Released
                    && self.durable_proof
                    && model::normal_release(&self.record)
            }
            // Original normal kernel facts can exist when the durable commit
            // fails. Preserve those observations without claiming release.
            wire::StatusPhase::ClosingUnconfirmed => {
                self.disposition == wire::OwnerDisposition::Retained
                    && (owner == OwnerPhase::Closing
                        || (owner == OwnerPhase::Released && !self.durable_proof))
            }
            _ => false,
        };
        if !valid {
            return Err("Contradictory supervision status".into());
        }
        if let Some(p) = &self.prepared {
            if self.phase != wire::StatusPhase::ClosingUnconfirmed
                || self.abnormal_protocol != 1
                || !self.durable_proof
                || !model::complete_abnormal_proof(&self.record)
                || &p.identity != model::identity(&self.record)?
                || &p.resources != model::facts(&self.record)?
                || !model::joined(&p.reader_joins)
                || self.record.reader_joins.as_ref() != Some(&p.reader_joins)
            {
                return Err("Unbound abnormal resource closure preparation".into());
            }
        }
        Ok(())
    }
}

fn identity(mut b: wire::identity::Builder<'_>, i: &Identity) {
    b.set_profile(&i.profile);
    b.set_generation(i.generation);
    b.set_incarnation(&i.incarnation);
    b.set_host_sha256(&i.host_sha256);
    b.set_package_sha256(&i.package_sha256);
    b.set_supervisor_sha256(&i.supervisor_sha256);
    b.set_supervisor_pid(i.supervisor_pid);
    b.set_supervisor_creation_filetime(i.supervisor_creation_filetime);
    b.set_child_pid(i.child_pid);
    b.set_child_creation_filetime(i.child_creation_filetime);
    b.set_recovery_history_head(&i.recovery_history_head);
}

fn facts(mut b: wire::resource_facts::Builder<'_>, f: &ResourceFacts) -> Result<()> {
    b.set_resource_reclaimed(f.resource_reclaimed);
    b.set_gate_closed(f.gate_closed);
    b.set_tree_empty(f.tree_empty);
    b.set_child_exit(f.child_exit);
    b.set_stdout_eof(f.stdout_eof);
    b.set_stderr_eof(f.stderr_eof);
    b.set_control_reaped(f.control_reaped);
    b.set_business_gate_revoked(f.business_gate_revoked);
    b.set_normal_shutdown(match ObservedTruth::try_from(f.normal_shutdown)? {
        ObservedTruth::TruthUnspecified => wire::ObservedTruth::Unspecified,
        ObservedTruth::No => wire::ObservedTruth::No,
        ObservedTruth::Yes => wire::ObservedTruth::Yes,
    });
    let mut exit = b.init_child_exit_code();
    match f.child_exit_code {
        Some(code) => exit.set_code(code),
        None => exit.set_absent(()),
    }
    Ok(())
}

fn joins(mut b: wire::reader_joins::Builder<'_>, j: &ReaderJoins) -> Result<()> {
    b.set_version(u16::try_from(j.version)?);
    b.set_stdout_reader_joined(j.stdout_reader_joined);
    b.set_stderr_reader_joined(j.stderr_reader_joined);
    b.set_controller_reader_joined(j.controller_reader_joined);
    Ok(())
}

fn owner(mut b: wire::owner_record::Builder<'_>, r: &OwnerRecord) -> Result<()> {
    model::validate(r)?;
    b.set_model_version(u16::try_from(r.model_version)?);
    identity(b.reborrow().init_identity(), model::identity(r)?);
    b.set_phase(match OwnerPhase::try_from(r.phase)? {
        OwnerPhase::Pending => wire::OwnerPhase::Pending,
        OwnerPhase::Active => wire::OwnerPhase::Active,
        OwnerPhase::Closing => wire::OwnerPhase::Closing,
        OwnerPhase::Released => wire::OwnerPhase::Released,
        OwnerPhase::Recovered => wire::OwnerPhase::Recovered,
        _ => return Err("Missing owner phase".into()),
    });
    facts(b.reborrow().init_resources(), model::facts(r)?)?;
    if r.business_outcome != BusinessOutcome::Unknown as i32
        || r.protocol_ack != ProtocolAck::NotApplicable as i32
    {
        return Err("Unsupported owner business outcome".into());
    }
    b.set_business_outcome(wire::BusinessOutcome::Unknown);
    b.set_protocol_ack(wire::ProtocolAck::NotApplicable);
    if let Some(j) = &r.reader_joins {
        joins(b.reborrow().init_reader_joins(), j)?;
    }
    if let Some(r) = &r.recovery {
        let mut binding = b.init_recovery();
        binding.set_version(u16::try_from(r.version)?);
        binding.set_original_digest(&r.original_digest);
        binding.set_receipt_digest(&r.receipt_digest);
        binding.set_history_head(&r.history_head);
    }
    Ok(())
}

fn status(mut b: wire::status::Builder<'_>, s: &Status) -> Result<()> {
    s.validate()?;
    owner(b.reborrow().init_record(), &s.record)?;
    b.set_phase(s.phase);
    b.set_owner_disposition(s.disposition);
    b.set_durable_proof(s.durable_proof);
    b.set_abnormal_resource_closure_protocol(s.abnormal_protocol);
    if let Some(p) = &s.prepared {
        let mut v = b.init_prepared();
        v.set_version(1);
        identity(v.reborrow().init_identity(), &p.identity);
        v.set_committed_record_sha256(&p.committed_digest);
        facts(v.reborrow().init_resources(), &p.resources)?;
        joins(v.reborrow().init_reader_joins(), &p.reader_joins)?;
        v.set_durable_abnormal_record_confirmed(true);
        v.set_owner_disposition(wire::OwnerDisposition::Retained);
        v.set_business_outcome(wire::BusinessOutcome::Unknown);
        v.set_protocol_ack(wire::ProtocolAck::NotApplicable);
        v.set_accepted(false);
        v.set_supervisor_exit_confirmed(false);
    }
    Ok(())
}

pub fn encode(value: Message<'_>) -> Result<Vec<u8>> {
    let mut message = capnp::message::Builder::new_default();
    let mut envelope = message.init_root::<wire::envelope::Builder<'_>>();
    envelope.set_wire_version(WIRE_VERSION);
    envelope.set_schema_sha256(SCHEMA_SHA256);
    match value {
        Message::Status(s) => status(envelope.init_status(), s)?,
        Message::Preview(p) => {
            model::validate(&p.record)?;
            if model::identity(&p.record)?.profile != p.profile
                || ![0, 32].contains(&p.history_head.len())
                || p.reason.is_empty()
                || p.reason.len() > 512
                || p.eligible != (p.reason == "eligible_resource_only")
                || p.resource_recovery_confirmed != (p.reason == "already_resource_recovered")
                || p.resource_recovery_confirmed != p.recovery_original_digest.is_some()
                || (p.eligible
                    && (p.record.phase != OwnerPhase::Closing as i32
                        || !model::complete_abnormal_proof(&p.record)))
                || (p.resource_recovery_confirmed && p.record.phase != OwnerPhase::Recovered as i32)
            {
                return Err("Invalid owner recovery preview".into());
            }
            let mut b = envelope.init_preview();
            b.set_profile(&p.profile);
            b.set_preview_token(&p.token);
            b.set_original_digest(&p.original_digest);
            b.set_history_head(&p.history_head);
            b.set_eligible(p.eligible);
            b.set_reason(p.reason.as_str());
            b.set_resource_recovery_confirmed(p.resource_recovery_confirmed);
            if let Some(d) = p.recovery_original_digest {
                b.set_recovery_original_digest(&d);
            }
            owner(b.reborrow().init_record(), &p.record)?;
            b.set_business_outcome(wire::BusinessOutcome::Unknown);
        }
        Message::Recovery(r) => {
            model::validate(&r.record)?;
            let binding = r
                .record
                .recovery
                .as_ref()
                .ok_or("Recovery binding missing")?;
            if r.record.phase != OwnerPhase::Recovered as i32
                || model::identity(&r.record)?.profile != r.profile
                || binding.original_digest != r.original_digest
                || binding.history_head != r.history_head
            {
                return Err("Invalid resource-only recovery result".into());
            }
            let mut b = envelope.init_recovery();
            b.set_profile(&r.profile);
            b.set_request_token(&r.request_token);
            b.set_original_digest(&r.original_digest);
            b.set_history_head(&r.history_head);
            b.set_recovered(true);
            b.set_business_outcome(wire::BusinessOutcome::Unknown);
            b.set_normal_shutdown(wire::ObservedTruth::No);
            owner(b.init_record(), &r.record)?;
        }
        Message::Failure { reason, detail } => {
            if reason.is_empty() || reason.len() > 512 || detail.len() > 8192 {
                return Err("Owner management failure bounds".into());
            }
            let mut b = envelope.init_failure();
            b.set_eligible(false);
            b.set_recovered(false);
            b.set_reason(reason);
            b.set_detail(detail);
            b.set_business_outcome(wire::BusinessOutcome::Unknown);
        }
    }
    let bytes = capnp::serialize::write_message_to_words(&message);
    if bytes.len() > MAX_PACKET {
        return Err("Supervisor message exceeds frame bound".into());
    }
    Ok(bytes)
}
