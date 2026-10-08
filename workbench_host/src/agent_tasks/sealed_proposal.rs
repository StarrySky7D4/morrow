//! Fixed9552 artifact on its explicit reviewed combined-import catalog profile.
//! The combined loader's narrow session route accepts canonical R2 frames only.
//! The retained guest admission has read/propose only; it never borrows executor authority.
use crate::Result;
use morrow_agent_catalog_admin_v1::Revisions;
use morrow_agent_catalog_owner_v1::CatalogOwner;
use morrow_agent_session_exec_v1_r2::{self as r2, authority::SessionExecHost};
use morrow_agent_session_process_v1_host::catalog::CatalogManagedPackage;
use morrow_core::{dispatch::HostRuntime, task::{Invocation, Transform}};
use morrow_plugin_runtime::{TaskRun, io_jobs::ManagedHostOwner, manager::Manager};
use std::sync::Arc;

const PROPOSAL_MODULE: [u8; 32] = [
    0x95,0x52,0xe9,0x69,0xb0,0x9c,0x9f,0x98,0x29,0x14,0xe7,0x1e,0xa9,0x9a,0x3c,0x93,
    0xfd,0xdc,0x2b,0xdd,0x30,0x98,0x6f,0x59,0x32,0x88,0x3b,0xca,0xa3,0xac,0x12,0x43,
];

/// Complete immutable wrapper identity plus the fixed actual input. This is a request
/// for a separately approved guest connection, never an admission or runtime handle.
pub struct SealedProposalSpec {
    id: String,
    wrapper: [u8; 32],
    base: [u8; 32],
    expected: Revisions,
    input: r2::Request,
    actual: r2::Request,
    task: Invocation,
    session: String,
    domain: String,
}
impl SealedProposalSpec {
    pub fn new(id: String, wrapper: [u8; 32], base: [u8; 32], expected: Revisions,
        input: r2::Request) -> Result<Self> {
        if id.is_empty() || id.len() > 256 || wrapper == [0; 32] || base == [0; 32]
            || expected.catalog == 0 || input.generation() == 0 {
            return Err("invalid sealed proposal identity".into());
        }
        let r2::Action::Propose { session_id, intent } = input.action() else {
            return Err("sealed proposal requires original Propose input".into());
        };
        input.action().validate().map_err(|e| format!("sealed proposal action: {e:?}"))?;
        let session = session_id.clone();
        let domain = intent.execution_domain.clone();
        let nonce = hex(&input.digest()[..16]);
        let actual = r2::Request::new_for_generation(format!("codex-{nonce}-1"),
            input.generation(), input.action().clone())
            .map_err(|e| format!("sealed proposal request: {e:?}"))?;
        let task = Invocation::new_transform(&format!("vm-proposal-{nonce}"), Transform {
            handler: "codex.session.propose".into(), input_type: "codex.session.proposal.v1".into(),
            output_type: "codex.session.proposal.receipt.v1".into(), input: input.raw().to_vec(),
        }).map_err(|e| format!("sealed proposal task: {e:?}"))?;
        Ok(Self { id, wrapper, base, expected, input, actual, task, session, domain })
    }
    pub fn revisions(&self) -> Revisions { self.expected }
    pub fn request_sha256(&self) -> [u8; 32] { self.actual.digest() }
    pub fn actual_request(&self) -> &r2::Request { &self.actual }
    pub fn generation(&self) -> u64 { self.input.generation() }
}

/// Retained privately until original worker cleanup after native facts and task join.
/// A completed VM call does not revoke its proposer: native R2 still checks that issuer.
pub(crate) struct SealedProposalLease {
    package: CatalogManagedPackage,
    spec: SealedProposalSpec,
    attempted: bool,
    completed: bool,
}
pub(crate) struct SealedProposalPrepareFailure {
    pub(crate) error: Box<dyn std::error::Error>,
    pub(crate) lease: Option<SealedProposalLease>,
}
impl SealedProposalLease {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn prepare(manager: &mut Manager, catalog: &mut CatalogOwner,
        runtime: &mut HostRuntime, host: &SessionExecHost, spec: SealedProposalSpec,
        expires: u64, now: u64) -> std::result::Result<Self, SealedProposalPrepareFailure> {
        // The immutable9552 module imports morrow_agent_session_process_v1.call.
        // Select that exact profile directly; never try/fallback from single-R2.
        let package = match catalog.connect(manager, runtime, host, &spec.id, spec.wrapper,
            spec.expected, expires, now) {
            Ok(package) => package,
            Err(error) => return Err(SealedProposalPrepareFailure {
                error: format!("original combined proposer catalog connection: {error:?}").into(), lease: None,
            }),
        };
        let checked: Result<()> = (|| {
            let p = package.package();
            let d = p.declaration();
            let s = d.session;
            let c = d.process;
            let admitted = package.session_capabilities();
            if p.review_sha256() != spec.wrapper || p.base_sha256() != spec.base
                || p.base().manifest().package_id != spec.id
                || p.base().module().len() != 442926 || r2::hash(p.base().module()) != PROPOSAL_MODULE
                || package.generation() != spec.generation()
                || !admitted.session_read || !admitted.propose
                || admitted.session_write || admitted.execute || admitted.retire
                || !s.session_read || !s.propose || s.session_write || s.execute || s.retire
                || c.read || c.events || c.write || c.close_input || c.interrupt || c.terminate || c.resize_pty
                || d.sessions != [spec.session.clone()] || d.execution_domain != spec.domain {
                return Err("sealed proposal module, full review, generation or declared ceiling mismatch".into());
            }
            Ok(())
        })();
        let lease = Self { package, spec, attempted: false, completed: false };
        match checked {
            Ok(()) => Ok(lease),
            Err(error) => { lease.request_stop(); Err(SealedProposalPrepareFailure { error, lease: Some(lease) }) }
        }
    }
    pub(crate) fn stop_handle(&self) -> Arc<dyn Fn() + Send + Sync> { self.package.stop_handle() }
    pub(crate) fn request_stop(&self) { self.package.request_stop(); }
    pub(crate) fn request_sha256(&self) -> [u8; 32] { self.spec.request_sha256() }
    pub(crate) fn revocation(&self, runtime: &HostRuntime) -> Result<morrow_core::lifecycle::Revocation> {
        self.package.revocation(runtime)
            .map_err(|e| format!("original sealed proposer revocation: {e:?}").into())
    }
    /// Revalidate the original proposer after Claim by a pure reader check.
    /// Its actual successful Propose reply already bound the original admission
    /// nonce: original R2 refuses a duplicate request from a different proposer.
    pub(crate) fn validate_live<O: ManagedHostOwner>(&self, owner: &O,
        host: &SessionExecHost, operation: &str, proposal_sha256: [u8; 32], now: u64) -> Result<()> {
        let r2::Action::Propose { intent, .. } = self.spec.actual.action() else {
            return Err("sealed proposal action unavailable".into());
        };
        if !self.completed || self.spec.actual.digest() != proposal_sha256
            || intent.operation_id != operation { return Err("original sealed proposer identity mismatch".into()); }
        let observation = self.package.validate_tool_observation(
            owner.manager().ok_or("original proposal manager unavailable")?, owner.runtime(),
            host, operation, proposal_sha256, now)
            .map_err(|e| format!("original sealed proposer no longer live: {e:?}"))?;
        if observation.identity.session_id != self.spec.session
            || observation.identity.intent_sha256 != intent.digest().map_err(|e| format!("intent digest: {e:?}"))? {
            return Err("original sealed proposal observation mismatch".into());
        }
        Ok(())
    }
    /// One real Wasm task. Any failed/lost result remains attempted and cannot replay.
    pub(crate) fn run_owned<O: ManagedHostOwner>(&mut self, owner: &mut O,
        host: &SessionExecHost, clock: impl FnMut() -> u64) -> Result<TaskRun> {
        if self.attempted { return Err("sealed proposal already attempted; Unknown cannot replay".into()); }
        self.attempted = true;
        // Only the combined profile's canonical-R2 route is exposed to this
        // retained proposer. It cannot route/register any process-control frame.
        let mut run = self.package.run_session_owned(owner, host, self.spec.task.bytes(), clock)
            .map_err(|e| format!("original sealed proposal task: {e:?}; no replay"))?;
        let verified: Result<()> = (|| {
            if run.report.outcome != Ok(0) || run.report.host_calls != 1 {
                return Err("sealed proposal VM did not complete Ok(0) with one import".into());
            }
            let completion = run.completion.as_ref().ok_or("sealed proposal completion absent")?;
            let output = self.spec.task.verify_output(completion)
                .map_err(|e| format!("sealed proposal task output: {e:?}"))?;
            let reply = r2::Reply::decode_for(&self.spec.actual, &output.bytes)
                .map_err(|e| format!("sealed proposal actual request correlation: {e:?}"))?;
            if !matches!(reply.outcome, r2::Outcome::Tool(info) if info.phase == r2::ToolPhase::Proposed) {
                return Err("sealed proposal original reply was not Proposed".into());
            }
            Ok(())
        })();
        if let Err(error) = verified {
            if let Some(bytes) = &mut run.completion { bytes.fill(0); }
            self.request_stop();
            return Err(error);
        }
        // Keep the actual read/propose connection live. Approval/Claim is a different
        // trusted command against the original executor and observed record revision.
        self.completed = true;
        Ok(run)
    }
    pub(crate) fn close(&self, runtime: &mut HostRuntime, host: &SessionExecHost) -> Result<()> {
        self.package.close_session(runtime, host).map_err(|e| format!("sealed proposer cleanup: {e:?}").into())
    }
}
fn hex(bytes: &[u8]) -> String { bytes.iter().map(|b| format!("{b:02x}")).collect() }
