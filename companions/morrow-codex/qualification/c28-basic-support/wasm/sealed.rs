//! Exact immutable guest inputs, actual receipt checks and separate trusted native decisions.
#![forbid(unsafe_code)]
use morrow_agent_session_exec_v1_r2 as r2;
use morrow_core::task::{Invocation, Transform};
use morrow_workbench_host::{Workbench, agent_tasks::{AgentCommand, AgentReply}, io_tasks::TaskKey};
use std::time::{Duration, Instant};

pub const PROPOSAL_SHA: &str = "9552e969b09c9f982914e71ea99a3c93fddc2bdd30986f5932883bcaa3ac1243";
pub const PROCESS_SHA: &str = "48e2dd065c0f6ec4971650dd34ec25f852469479344727bef93f5f90334e1b36";
pub const SESSION_SHA: &str = "b1f0f44ad2bbe2abc89eee0ee971683419bf3407f370194f5557df57fa45799e";

/// Separately derived public fixture; never an alias for historical SESSION_SHA.
pub const PUBLIC_SESSION_R2_V1_SHA: &str = "cca04ebb2e787f69e84ec7260aca3e93ec895ec17b68afbb660e3c6896ae2f2b";
pub const PUBLIC_SESSION_R2_V1_BYTES: usize = 425912;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum GuestIdentity {
    HistoricalProposal,
    HistoricalProcess,
    HistoricalSession,
    PublicSessionR2V1,
}

pub fn check_public_session_r2_v1(bytes: &[u8]) -> Result<(), String> {
    if bytes.len() != PUBLIC_SESSION_R2_V1_BYTES || !bytes.starts_with(b"\0asm\x01\0\0\0")
        || crate::witness::hex(&r2::hash(bytes)) != PUBLIC_SESSION_R2_V1_SHA {
        return Err("immutable public session r2 v1 identity mismatch".into());
    }
    Ok(())
}

pub(crate) fn check_selected_guest(bytes: &[u8], identity: GuestIdentity) -> Result<(), String> {
    match identity {
        GuestIdentity::HistoricalProposal => check_guest(bytes, PROPOSAL_SHA),
        GuestIdentity::HistoricalProcess => check_guest(bytes, PROCESS_SHA),
        GuestIdentity::HistoricalSession => check_guest(bytes, SESSION_SHA),
        GuestIdentity::PublicSessionR2V1 => check_public_session_r2_v1(bytes),
    }
}

pub fn check_guest(bytes: &[u8], expected: &str) -> Result<(), String> {
    if ![PROPOSAL_SHA, PROCESS_SHA, SESSION_SHA].contains(&expected)
        || bytes.len() > 4 * 1024 * 1024 || !bytes.starts_with(b"\0asm\x01\0\0\0")
        || crate::witness::hex(&r2::hash(bytes)) != expected {
        return Err("immutable sealed guest identity mismatch".into());
    }
    Ok(())
}

#[derive(Debug)]
pub struct SessionEvidence {
    pub generation: u64,
    pub checkpoint_sha256: [u8; 32],
    pub parent_tail: u64,
}

/// Requires a separately reviewed session-only wrapper scoped to sid and sid-child.
/// Seven original R2 calls; no native execution or implicit approval is performed.
pub fn run_combined_session(workbench: &Workbench, key: TaskKey, generation: u64, nonce: [u8; 16], sid: &str) -> Result<SessionEvidence, String> {
    if generation == 0 || nonce == [0; 16] { return Err("invalid session guest identity".into()) }
    for name in [sid.to_owned(), format!("{sid}-child")] {
        r2::Action::Create { session_id: name, parent: None, parent_tail: 0 }.validate().map_err(|e| e.to_string())?;
    }
    let mut input = generation.to_le_bytes().to_vec();
    input.extend_from_slice(&nonce);
    input.extend_from_slice(sid.as_bytes());
    if input.len() > 280 { return Err("session guest input limit".into()) }
    let task = Invocation::new_transform(&format!("vm-session-{}", crate::witness::hex(&nonce)), Transform {
        handler: "codex.session.continue".into(), input_type: "codex.session.config.v1".into(), output_type: "codex.session.receipt.v1".into(), input,
    }).map_err(|e| format!("session task: {e:?}"))?;
    let raw = crate::controls::run_task(workbench, key, &task, 7, Instant::now() + Duration::from_secs(20))?;
    parse_session_receipt(&raw, generation)
}

/// Parse the existing receipt format only; no host import or authority operation.
fn parse_session_receipt(raw: &[u8], generation: u64) -> Result<SessionEvidence, String> {
    if raw.len() != 48 { return Err("session receipt length".into()) }
    let got_generation = u64::from_le_bytes(raw[..8].try_into().map_err(|_| "generation length")?);
    let checkpoint_sha256 = raw[8..40].try_into().map_err(|_| "checkpoint length")?;
    let parent_tail = u64::from_le_bytes(raw[40..].try_into().map_err(|_| "tail length")?);
    if got_generation != generation || checkpoint_sha256 != r2::hash(b"sealed-state\0\xff") || parent_tail != 1 {
        return Err("session receipt generation/checkpoint/tail mismatch".into());
    }
    Ok(SessionEvidence { generation, checkpoint_sha256, parent_tail })
}

/// Produces the original proposal task plus the actual guest-generated request.
/// Actual trusted review/approve/Claim uses the separate original-owner command lane below.
/// Keep the original proposer live through execution and final facts maintenance.
pub fn proposal_task(input: &r2::Request) -> Result<(Invocation, r2::Request), String> {
    if !matches!(input.action(), r2::Action::Propose { .. }) { return Err("proposal action required".into()) }
    let digest = input.digest();
    let nonce = &digest[..16];
    let actual = r2::Request::new_for_generation(format!("codex-{}-1", crate::witness::hex(nonce)), input.generation(), input.action().clone())
        .map_err(|e| e.to_string())?;
    let task = proposal_invocation(input)?;
    Ok((task, actual))
}
fn proposal_invocation(input: &r2::Request) -> Result<Invocation, String> {
    if !matches!(input.action(), r2::Action::Propose { .. }) { return Err("proposal action required".into()) }
    Invocation::new_transform(&format!("vm-proposal-{}", crate::witness::hex(&input.digest()[..16])), Transform {
        handler: "codex.session.propose".into(), input_type: "codex.session.proposal.v1".into(), output_type: "codex.session.proposal.receipt.v1".into(), input: input.raw().to_vec(),
    }).map_err(|e| format!("proposal task: {e:?}"))
}

pub fn verify_proposal(actual: &r2::Request, raw: &[u8]) -> Result<r2::Reply, String> {
    let reply = r2::Reply::decode_for(actual, raw).map_err(|e| e.to_string())?;
    if !matches!(&reply.outcome, r2::Outcome::Tool(info) if info.phase == r2::ToolPhase::Proposed) {
        return Err("actual proposal was not Proposed".into());
    }
    Ok(reply)
}

/// Data from one completed original Wasm task. It is not approval or execution
/// authority. The original host must independently review its durable record.
pub struct GuestProposal {
    actual: r2::Request,
    intent: r2::Intent,
    session: String,
    review: Option<(r2::safe_exec::ToolReview, r2::safe_exec::ToolObservation)>,
    phase: ProposalPhase,
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum ProposalPhase { Proposed, Reviewed, Unknown, Claimed }

/// The one-shot original claim remains separate from a child start. No Clone is
/// provided. Loss of this delivery never authorizes another approval or Claim.
pub struct GuestClaim {
    pub observation: r2::safe_exec::ToolObservation,
    pub claim: [u8; 32],
}
impl GuestProposal {
    /// Call only on the original fixed-R2 profile's completed TaskRun. The VM
    /// execution must already be over; a queued command or bare reply is not proof.
    pub fn from_completed_task(input: &r2::Request, run: &morrow_plugin_runtime::TaskRun) -> Result<Self, String> {
        let (_, actual) = proposal_task(input)?;
        Self::from_completed_actual_task(input, &actual, run)
    }
    /// The formal caller clones this canonical actual request from the opaque
    /// original-owner Spec before preparation consumes it; no generated-name mirror.
    pub fn from_completed_actual_task(input: &r2::Request, actual: &r2::Request, run: &morrow_plugin_runtime::TaskRun) -> Result<Self, String> {
        if actual.generation() != input.generation() || actual.action() != input.action() {
            return Err("actual proposal input differs from original prepared Spec".into());
        }
        let task = proposal_invocation(input)?;
        if run.report.outcome != Ok(0) || run.report.host_calls != 1 {
            return Err("sealed proposal guest did not finish Ok(0) with exactly one import; no retry".into());
        }
        let completion = run.completion.as_ref().ok_or("sealed proposal completion absent; Unknown")?;
        let output = task.verify_output(completion).map_err(|e| format!("actual proposal task correlation: {e:?}"))?;
        if output.bytes.len() > r2::MAX_FRAME_BYTES { return Err("proposal reply limit; Unknown".into()) }
        verify_proposal(actual, &output.bytes)?;
        let r2::Action::Propose { session_id, intent } = actual.action() else { return Err("proposal action required".into()) };
        Ok(Self { session: session_id.clone(), intent: intent.clone(), actual: actual.clone(), review: None, phase: ProposalPhase::Proposed })
    }
    /// One original Worker call to its retained, catalog-approved fixed R2 guest.
    /// Submission or lost delivery is never retried by this caller or the lease.
    pub fn run_once(workbench: &Workbench, key: TaskKey, input: &r2::Request,
        actual: &r2::Request) -> Result<Self, String> {
        if actual.generation() != input.generation() || actual.action() != input.action() {
            return Err("actual proposal input differs from original prepared Spec".into());
        }
        let mut handle = workbench.submit_agent(key, AgentCommand::RunSealedProposal {
            request_sha256: actual.digest(),
        }).map_err(|e| format!("original sealed proposal submission rejected: {e:?}; no retry"))?;
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            match handle.try_read().map_err(|e| format!("actual sealed proposal delivery Unknown: {e:?}"))? {
                Some(reply) => {
                    let AgentReply::Frame(run) = &reply else { return Err("unexpected sealed proposal reply; Unknown".into()) };
                    return Self::from_completed_actual_task(input, actual, run);
                }
                None if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(5)),
                None => { handle.cancel(); return Err("actual sealed proposal deadline; Unknown; cleanup required".into()) }
            }
        }
    }
    pub fn proposal_sha256(&self) -> [u8; 32] { self.actual.digest() }
    pub fn generation(&self) -> u64 { self.actual.generation() }
    pub fn intent(&self) -> &r2::Intent { &self.intent }
    pub fn session(&self) -> &str { &self.session }
    pub fn unknown(&self) -> bool { self.phase == ProposalPhase::Unknown }

    /// One explicit original host read. Unexpected or lost delivery stays
    /// latched, rather than recycling a request or refreshing into permission.
    pub fn review_once(&mut self, workbench: &Workbench, key: TaskKey) -> Result<(), String> {
        if self.phase != ProposalPhase::Proposed { return Err("proposal review not available; no replay".into()) }
        self.phase = ProposalPhase::Unknown;
        let mut handle = workbench.submit_agent(key, AgentCommand::ReviewTool { operation_id: self.intent.operation_id.clone() })
            .map_err(|e| format!("original trusted review rejected: {e:?}"))?;
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            match handle.try_read().map_err(|e| format!("original review delivery Unknown: {e:?}"))? {
                Some(reply) => {
                    let AgentReply::ToolReview { review, observation } = &reply else { return Err("unexpected review reply; Unknown".into()) };
                    let identity = &observation.identity;
                    if review.proposal_sha256 != self.actual.digest() || review.intent_sha256 != self.intent.digest().map_err(|e| e.to_string())?
                        || review.intent != self.intent || review.session_id != self.session || review.session_epoch != identity.session_epoch
                        || review.expires_ms == 0 || identity.proposal_sha256 != self.actual.digest()
                        || identity.intent_sha256 != review.intent_sha256 || identity.operation_id != self.intent.operation_id
                        || identity.execution_domain != self.intent.execution_domain || identity.session_id != self.session
                        || identity.generation != self.actual.generation() || identity.original_issuer_nonce == [0; 32]
                        || observation.record_revision == 0 || observation.record_sha256 == [0; 32]
                        || observation.phase != r2::ToolPhase::Proposed || observation.invocation_started
                        || observation.latest.is_some() || observation.accepted_report.is_some() {
                        return Err("original durable review differs from actual guest proposal; no approval".into());
                    }
                    self.review = Some((review.clone(), observation.clone()));
                    self.phase = ProposalPhase::Reviewed;
                    return Ok(());
                }
                None if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(5)),
                None => { handle.cancel(); return Err("original review deadline; Unknown; cleanup required".into()) }
            }
        }
    }

    /// Trusted-native approval and Claim are one single-delivery command. The
    /// expected original record revision/hash is checked before either mutation.
    /// The guest never receives this command, the permit, or execute admission.
    pub fn approve_claim_once(&mut self, workbench: &Workbench, key: TaskKey) -> Result<GuestClaim, String> {
        if self.phase != ProposalPhase::Reviewed { return Err("proposal is not freshly reviewed; no replay".into()) }
        self.phase = ProposalPhase::Unknown;
        let (_, prior) = self.review.as_ref().ok_or("original review missing; Unknown")?;
        let request_id = format!("vm-claim-{}-1", crate::witness::hex(&self.actual.digest()[..16]));
        let mut handle = workbench.submit_agent(key, AgentCommand::ApproveClaim {
            request_id, operation_id: self.intent.operation_id.clone(), proposal_sha256: self.actual.digest(),
            intent_sha256: self.intent.digest().map_err(|e| e.to_string())?,
            expected_record_revision: prior.record_revision, expected_record_sha256: prior.record_sha256,
        }).map_err(|e| format!("original approval/Claim submission rejected: {e:?}; no replay"))?;
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            match handle.try_read().map_err(|e| format!("original approval/Claim delivery Unknown: {e:?}"))? {
                Some(reply) => {
                    let AgentReply::Claimed { observation, claim } = &reply else { return Err("unexpected approval/Claim reply; Unknown".into()) };
                    if observation.identity != prior.identity || observation.phase != r2::ToolPhase::DispatchUnknown
                        || observation.invocation_started || observation.latest.is_some() || observation.accepted_report.is_some()
                        || observation.record_revision != prior.record_revision.checked_add(2).ok_or("record revision overflow")?
                        || observation.record_sha256 == [0; 32] || observation.record_sha256 == prior.record_sha256 || *claim == [0; 32] {
                        return Err("original post-Claim identity/revision mismatch; Unknown; no replay".into());
                    }
                    self.phase = ProposalPhase::Claimed;
                    return Ok(GuestClaim { observation: observation.clone(), claim: *claim });
                }
                None if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(5)),
                None => { handle.cancel(); return Err("original approval/Claim deadline; Unknown; cleanup required".into()) }
            }
        }
    }
}

#[cfg(test)]
mod receipt_tests {
    use super::parse_session_receipt;
    use morrow_agent_session_exec_v1_r2 as r2;

    fn original_receipt(generation: u64) -> Vec<u8> {
        let mut raw = generation.to_le_bytes().to_vec();
        raw.extend_from_slice(&r2::hash(b"sealed-state\0\xff"));
        raw.extend_from_slice(&1u64.to_le_bytes());
        raw
    }

    #[test]
    fn exact_original_receipt_is_accepted() {
        let evidence = parse_session_receipt(&original_receipt(7), 7).unwrap();
        assert_eq!(evidence.generation, 7);
        assert_eq!(evidence.checkpoint_sha256, r2::hash(b"sealed-state\0\xff"));
        assert_eq!(evidence.parent_tail, 1);
    }

    #[test]
    fn short_and_long_receipts_are_rejected_before_field_decode() {
        let raw = original_receipt(7);
        assert_eq!(parse_session_receipt(&raw[..47], 7).unwrap_err(), "session receipt length");
        let mut long = raw;
        long.push(0);
        assert_eq!(parse_session_receipt(&long, 7).unwrap_err(), "session receipt length");
    }

    #[test]
    fn wrong_generation_is_rejected() {
        for generation in [0, 8, u64::MAX] {
            assert!(parse_session_receipt(&original_receipt(generation), 7).is_err());
        }
    }

    #[test]
    fn one_bit_checkpoint_digest_mismatch_is_rejected() {
        let mut raw = original_receipt(7);
        raw[8] ^= 1;
        assert!(parse_session_receipt(&raw, 7).is_err());
    }

    #[test]
    fn zero_two_and_maximum_parent_tails_are_rejected() {
        for tail in [0u64, 2, u64::MAX] {
            let mut raw = original_receipt(7);
            raw[40..48].copy_from_slice(&tail.to_le_bytes());
            assert!(parse_session_receipt(&raw, 7).is_err());
        }
    }

    #[test]
    fn swapped_checkpoint_and_tail_fields_are_rejected() {
        let raw = original_receipt(7);
        let mut swapped = raw[..8].to_vec();
        swapped.extend_from_slice(&raw[40..48]);
        swapped.extend_from_slice(&raw[8..40]);
        assert_eq!(swapped.len(), 48);
        assert!(parse_session_receipt(&swapped, 7).is_err());
    }
}


#[cfg(test)]
mod public_session_identity_tests {
    use super::*;

    const PUBLIC: &[u8] = include_bytes!(
        "../../c28-basic-fixtures/public-session-r2-v1/morrow_codex_session_exec_guest_r2.wasm"
    );

    #[test]
    fn public_session_r2_v1_exact_bytes_select_only_new_identity() {
        assert_eq!(PUBLIC.len(), PUBLIC_SESSION_R2_V1_BYTES);
        check_public_session_r2_v1(PUBLIC).unwrap();
        check_selected_guest(PUBLIC, GuestIdentity::PublicSessionR2V1).unwrap();
        for identity in [GuestIdentity::HistoricalProposal, GuestIdentity::HistoricalProcess,
                         GuestIdentity::HistoricalSession] {
            assert!(check_selected_guest(PUBLIC, identity).is_err());
        }
    }

    #[test]
    fn legacy_hash_entrypoint_does_not_admit_public_identity() {
        assert_eq!(SESSION_SHA, "b1f0f44ad2bbe2abc89eee0ee971683419bf3407f370194f5557df57fa45799e");
        assert!(check_guest(PUBLIC, SESSION_SHA).is_err());
        assert!(check_guest(PUBLIC, PUBLIC_SESSION_R2_V1_SHA).is_err());
        assert!(check_guest(PUBLIC, "unknown").is_err());
    }

    #[test]
    fn public_identity_rejects_short_and_appended_bytes() {
        assert!(check_public_session_r2_v1(&PUBLIC[..PUBLIC.len() - 1]).is_err());
        let mut appended = PUBLIC.to_vec();
        appended.push(0);
        assert!(check_public_session_r2_v1(&appended).is_err());
    }

    #[test]
    fn public_identity_rejects_same_length_payload_and_magic_changes() {
        let mut payload = PUBLIC.to_vec();
        let last = payload.len() - 1;
        payload[last] ^= 1;
        assert!(check_public_session_r2_v1(&payload).is_err());
        let mut magic = PUBLIC.to_vec();
        magic[0] ^= 1;
        assert!(check_public_session_r2_v1(&magic).is_err());
    }
}
