//! Opaque original-owner preparation for immutable guests. No host Propose,
//! Create, Claim or child start is dispatched by either helper in this module.
use anyhow::{Result, anyhow, ensure};
use morrow_agent_session_exec_v1_r2::{
    Action, Intent, Request,
    authority::{Capabilities, SessionExecHost},
};
use morrow_codex_session_exec_windows_v1::{
    BorrowedNativeResources, ExecParams, ProvisionedWindowsBackend, ReviewedBorrowedInvocation,
};
use morrow_workbench_host::{
    Workbench,
    agent_tasks::{AgentContext, SealedProposalSpec},
};
use std::sync::Arc;

pub struct PreparedSessionGuest {
    pub context: Option<AgentContext>,
    pub host: Arc<SessionExecHost>,
    pub generation: u64,
}

/// Only the independently approved b1f session wrapper may use this context.
/// Its immutable guest performs the actual seven session actions later.
pub fn prepare_session_guest(workbench: &mut Workbench, sid: &str) -> Result<PreparedSessionGuest> {
    let child = format!("{sid}-child");
    for session_id in [sid, child.as_str()] {
        Action::Create {
            session_id: session_id.into(),
            parent: None,
            parent_tail: 0,
        }
        .validate()?;
    }
    let mut scope = vec![sid.to_owned(), child];
    scope.sort();
    let mut captured = None;
    let context = workbench
        .prepare_agent_context(|owner| {
            let host = owner.session_host()?;
            let connection = owner.connect()?;
            let caps = Capabilities {
                session_read: true,
                session_write: true,
                ..Default::default()
            };
            let expires = owner.expires_after(60_000)?;
            // This tracked original host grant is not the guest's catalog grant.
            owner.admit(
                &host,
                &connection,
                caps,
                caps,
                scope,
                "session-only".into(),
                expires,
            )?;
            let generation = owner.generation(&host)?;
            captured = Some((host, generation));
            owner.context(Default::default(), None)
        })
        .map_err(|e| anyhow!("original session guest context preparation: {e}"))?;
    let (host, generation) =
        captured.ok_or_else(|| anyhow!("original session context capture absent"))?;
    Ok(PreparedSessionGuest {
        context: Some(context),
        host,
        generation,
    })
}

pub struct PreparedNativeProposal {
    pub context: Option<AgentContext>,
    pub host: Arc<SessionExecHost>,
    pub resources: Arc<BorrowedNativeResources>,
    pub reviewed: Option<ReviewedBorrowedInvocation>,
    pub intent: Intent,
    /// Canonical input to the real 9552 guest, not a durable host proposal.
    /// The immutable guest derives its own correlated request ID from this input.
    pub input: Request,
    /// Cloned from the opaque Spec before consumption, never a caller-generated
    /// replacement frame for the retained guest connection.
    pub actual: Request,
}

/// Real native review/domain plus one original read/execute grant. The sid must
/// already have been created by the separately approved b1f guest and preserved
/// across that Worker's real join. Approval and Claim happen in later explicit
/// trusted Worker commands while its independent proposer lease is still live.
pub fn prepare_native_proposal(
    workbench: &mut Workbench,
    backend: Arc<ProvisionedWindowsBackend>,
    sid: &str,
    operation: &str,
    input_id: &str,
    params: ExecParams,
    helper_sha256: [u8; 32],
    expected_intent: &Intent,
    proposal_wrapper: &crate::wrapper::ReviewedWrapper,
    pair: morrow_agent_catalog_admin_v1::Revisions,
) -> Result<PreparedNativeProposal> {
    ensure!(
        backend.is_production(),
        "no ordinary backend for the formal guest path"
    );
    // Select the exact immutable9552 combined-import review. No executor
    // capability is borrowed by its separately retained read/propose lease.
    proposal_wrapper.require_sealed_proposal_review(sid, &expected_intent.execution_domain)?;
    let mut captured = None;
    let context = workbench
        .prepare_agent_context(|owner| {
            let host = owner.session_host()?;
            let resources = owner.resources()?;
            let port = owner.bind_port(host.clone(), resources.clone(), backend)?;
            let reviewed =
                port.review_fixed(operation.to_owned(), params, helper_sha256, 30_000)?;
            let intent = reviewed.intent().clone();
            if intent != *expected_intent {
                return Err(
                    "actual same-owner native review differs from explicitly approved intent"
                        .into(),
                );
            }
            let connection = owner.connect()?;
            let caps = Capabilities {
                session_read: true,
                execute: true,
                ..Default::default()
            };
            let expires = owner.expires_after(60_000)?;
            owner.admit(
                &host,
                &connection,
                caps,
                caps,
                vec![sid.to_owned()],
                intent.execution_domain.clone(),
                expires,
            )?;
            let generation = owner.generation(&host)?;
            // Pure canonical input only. No owner.dispatch is called, so a new
            // record/permit cannot be substituted for the immutable guest's effect.
            let input = Request::new_for_generation(
                input_id,
                generation,
                Action::Propose {
                    session_id: sid.to_owned(),
                    intent: intent.clone(),
                },
            )?;
            let spec = SealedProposalSpec::new(
                proposal_wrapper.id.clone(),
                proposal_wrapper.full_sha256,
                proposal_wrapper.base_sha256,
                pair,
                input.clone(),
            )?;
            let actual = spec.actual_request().clone();
            owner.prepare_sealed_proposal(spec, expires)?;
            captured = Some((host, resources, reviewed, intent, input, actual));
            owner.context(Default::default(), Some(port))
        })
        .map_err(|e| anyhow!("original native proposal context preparation: {e}"))?;
    let (host, resources, reviewed, intent, input, actual) =
        captured.ok_or_else(|| anyhow!("native proposal capture absent"))?;
    Ok(PreparedNativeProposal {
        context: Some(context),
        host,
        resources,
        reviewed: Some(reviewed),
        intent,
        input,
        actual,
    })
}
