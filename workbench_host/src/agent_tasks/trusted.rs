//! Trusted-native review and one-shot Claim on the still-exclusive original owner.
//! Frozen Request owns additional unerasable copies; this module does not promise
//! complete protocol-buffer erasure or change the frozen R2 wire/types.
use super::{AgentContext, AgentError, AgentReply};
use morrow_agent_session_exec_v1_r2::{
    Action, Intent, Outcome, Reply, Request, ToolPhase,
    safe_exec::{ToolObservation, ToolReview},
};
use morrow_plugin_runtime::io_jobs::ManagedHostOwner;
use zeroize::{Zeroize, Zeroizing};

/// Erase the new host-side full intent, including arguments and environment values.
pub(super) fn erase_intent(intent: &mut Intent) {
    intent.operation_id.zeroize();
    intent.artifact_sha256.zeroize();
    intent.program.zeroize();
    intent.argv.zeroize();
    intent.cwd.zeroize();
    for variable in &mut intent.env {
        variable.name.zeroize();
        variable.value.zeroize();
    }
    intent.env.clear();
    intent.input.zeroize();
    intent.execution_domain.zeroize();
    intent.max_runtime_ms.zeroize();
}
pub(super) fn erase_review(review: &mut ToolReview) {
    erase_intent(&mut review.intent);
    review.proposal_sha256.zeroize();
    review.intent_sha256.zeroize();
    review.session_id.zeroize();
    review.session_epoch.zeroize();
    review.expires_ms.zeroize();
}
struct SensitiveReview(Option<ToolReview>);
impl SensitiveReview {
    fn get(&self) -> &ToolReview {
        self.0.as_ref().expect("owned sensitive review")
    }
    fn deliver(mut self) -> ToolReview {
        self.0.take().expect("one review delivery")
    }
}
impl Drop for SensitiveReview {
    fn drop(&mut self) {
        if let Some(review) = &mut self.0 {
            erase_review(review);
        }
    }
}
struct SensitiveIntent(Intent);
impl Drop for SensitiveIntent {
    fn drop(&mut self) {
        erase_intent(&mut self.0);
    }
}
fn operation(value: &str) -> Result<(), AgentError> {
    Action::Inspect {
        operation_id: value.to_owned(),
    }
    .validate()
    .map_err(|_| AgentError::Invalid)
}
fn read_live<O: ManagedHostOwner>(
    owner: &O,
    context: &AgentContext,
    operation_id: &str,
    checkpoint: &dyn Fn() -> u64,
) -> Result<(), AgentError> {
    context
        .host
        .inspect_tool_observation(
            owner.runtime(),
            &context.executor_connection,
            &context.executor_admission,
            operation_id,
            checkpoint(),
        )
        .map(|_| ())
        .map_err(|_| AgentError::Invalid)
}
fn same_record<O: ManagedHostOwner>(
    owner: &O,
    context: &AgentContext,
    operation_id: &str,
    expected: &ToolObservation,
) -> Result<(), AgentError> {
    let current = context
        .host
        .inspect_tool_record(owner.runtime(), operation_id)
        .map_err(|_| AgentError::Unknown)?;
    if &current != expected {
        return Err(AgentError::Unknown);
    }
    Ok(())
}
pub(super) fn review<O: ManagedHostOwner>(
    owner: &mut O,
    context: &AgentContext,
    operation_id: &str,
    checkpoint: &dyn Fn() -> u64,
) -> Result<AgentReply, AgentError> {
    operation(operation_id)?;
    owner.prepare_io().map_err(|_| AgentError::Maintenance)?;
    read_live(owner, context, operation_id, checkpoint)?;
    let review = SensitiveReview(Some(
        context
            .host
            .review_tool(owner.runtime(), operation_id, checkpoint())
            .map_err(|_| AgentError::Invalid)?,
    ));
    let observation = context
        .host
        .inspect_tool_record(owner.runtime(), operation_id)
        .map_err(|_| AgentError::Unknown)?;
    let full = review.get();
    if observation.phase != ToolPhase::Proposed
        || observation.invocation_started
        || observation.identity.proposal_sha256 != full.proposal_sha256
        || observation.identity.intent_sha256 != full.intent_sha256
        || observation.identity.session_id != full.session_id
        || observation.identity.session_epoch != full.session_epoch
    {
        return Err(AgentError::Unknown);
    }
    owner.finish_io().map_err(|_| AgentError::Maintenance)?;
    read_live(owner, context, operation_id, checkpoint)?;
    // This original API checks the live proposer and active session again. The
    // duplicate review is sensitive too and is erased on every exit path.
    let fresh = SensitiveReview(Some(
        context
            .host
            .review_tool(owner.runtime(), operation_id, checkpoint())
            .map_err(|_| AgentError::Invalid)?,
    ));
    if fresh.get().intent != full.intent
        || fresh.get().proposal_sha256 != full.proposal_sha256
        || fresh.get().intent_sha256 != full.intent_sha256
        || fresh.get().session_id != full.session_id
        || fresh.get().session_epoch != full.session_epoch
        || fresh.get().expires_ms != full.expires_ms
    {
        return Err(AgentError::Unknown);
    }
    same_record(owner, context, operation_id, &observation)?;
    context.validate_sealed_proposer(owner, operation_id, full.proposal_sha256, checkpoint)?;
    read_live(owner, context, operation_id, checkpoint)?;
    Ok(AgentReply::ToolReview {
        review: review.deliver(),
        observation,
    })
}
#[allow(clippy::too_many_arguments)]
pub(super) fn approve_claim<O: ManagedHostOwner>(
    owner: &mut O,
    context: &AgentContext,
    request_id: &str,
    operation_id: &str,
    proposal_sha256: [u8; 32],
    intent_sha256: [u8; 32],
    expected_record_revision: u64,
    expected_record_sha256: [u8; 32],
    checkpoint: &dyn Fn() -> u64,
) -> Result<AgentReply, AgentError> {
    operation(operation_id)?;
    owner.prepare_io().map_err(|_| AgentError::Maintenance)?;
    read_live(owner, context, operation_id, checkpoint)?;
    let before = context
        .host
        .inspect_tool_record(owner.runtime(), operation_id)
        .map_err(|_| AgentError::Unknown)?;
    if before.phase != ToolPhase::Proposed
        || before.invocation_started
        || before.record_revision != expected_record_revision
        || before.record_sha256 != expected_record_sha256
        || before.identity.proposal_sha256 != proposal_sha256
        || before.identity.intent_sha256 != intent_sha256
    {
        return Err(AgentError::Invalid);
    }
    let generation = context
        .host
        .generation(owner.runtime())
        .map_err(|_| AgentError::Unknown)?;
    if before.identity.generation != generation {
        return Err(AgentError::Invalid);
    }
    Request::new_for_generation(
        request_id,
        generation,
        Action::Claim {
            operation_id: operation_id.to_owned(),
            permit: [1; 32],
        },
    )
    .map_err(|_| AgentError::Invalid)?;
    let permit = Zeroizing::new(
        context
            .host
            .approve(
                owner.runtime_mut(),
                &context.executor_connection,
                &context.executor_admission,
                operation_id,
                proposal_sha256,
                intent_sha256,
                checkpoint(),
            )
            .map_err(|_| AgentError::Unknown)?,
    );
    // Frozen Request retains its Action permit and encoded Vec until ordinary
    // drop. Those private buffers are not erasable through its public API.
    let request = Request::new_for_generation(
        request_id,
        generation,
        Action::Claim {
            operation_id: operation_id.to_owned(),
            permit: *permit,
        },
    )
    .map_err(|_| AgentError::Unknown)?;
    let raw = Zeroizing::new(
        context
            .host
            .dispatch(
                owner.runtime_mut(),
                &context.executor_connection,
                &context.executor_admission,
                request.raw(),
                checkpoint,
            )
            .map_err(|_| AgentError::Unknown)?,
    );
    let answer = Reply::decode_for(&request, &raw).map_err(|_| AgentError::Unknown)?;
    let claim = match answer.outcome {
        Outcome::Claimed {
            info,
            intent,
            mut claim,
        } => {
            let intent = SensitiveIntent(intent);
            let guarded_claim = Zeroizing::new(claim);
            claim.zeroize();
            if info.operation_id != operation_id
                || info.phase != ToolPhase::DispatchUnknown
                || intent.0.digest().ok() != Some(intent_sha256)
            {
                return Err(AgentError::Unknown);
            }
            guarded_claim
        }
        _ => return Err(AgentError::Unknown),
    };
    let observation = context
        .host
        .inspect_tool_record(owner.runtime(), operation_id)
        .map_err(|_| AgentError::Unknown)?;
    if observation.identity != before.identity
        || observation.phase != ToolPhase::DispatchUnknown
        || observation.invocation_started
        || observation.record_revision <= before.record_revision
    {
        return Err(AgentError::Unknown);
    }
    owner.finish_io().map_err(|_| AgentError::Unknown)?;
    // Claim is consumed. Every subsequent loss is Unknown; no approval, Claim,
    // start or other mutation is dispatched to perform these read validations.
    read_live(owner, context, operation_id, checkpoint).map_err(|_| AgentError::Unknown)?;
    same_record(owner, context, operation_id, &observation)?;
    context
        .validate_sealed_proposer(owner, operation_id, proposal_sha256, checkpoint)
        .map_err(|_| AgentError::Unknown)?;
    read_live(owner, context, operation_id, checkpoint).map_err(|_| AgentError::Unknown)?;
    Ok(AgentReply::Claimed {
        observation,
        claim: *claim,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use morrow_agent_process_control_v1::host::Host as ProcessHost;
    use morrow_agent_session_exec_v1_r2::{
        Environment,
        authority::{Admission, Capabilities, SessionExecHost},
        hash,
    };
    use morrow_codex_session_exec_windows_v1::BorrowedNativeResources;
    use morrow_core::{
        dispatch::HostRuntime,
        lifecycle::Revocation,
        store::{EventBudget, Store},
    };
    use morrow_plugin_runtime::{
        io_jobs::{HostOwner, JobError},
        manager::Manager,
    };
    use std::sync::Arc;

    const OPERATION: &str = "trusted-synthetic-operation";
    struct OrdinaryOwner {
        runtime: HostRuntime,
        revoke_on_finish: Option<Revocation>,
        fail_finish: bool,
        finishes: usize,
        _temp: tempfile::TempDir,
    }
    impl HostOwner for OrdinaryOwner {
        fn runtime(&self) -> &HostRuntime {
            &self.runtime
        }
        fn runtime_mut(&mut self) -> &mut HostRuntime {
            &mut self.runtime
        }
        fn finish_io(&mut self) -> Result<(), JobError> {
            self.finishes += 1;
            if let Some(revocation) = self.revoke_on_finish.take() {
                revocation.revoke();
            }
            if self.fail_finish {
                return Err(JobError::Unavailable);
            }
            Ok(())
        }
    }
    impl ManagedHostOwner for OrdinaryOwner {
        fn manager(&self) -> Option<&Manager> {
            None
        }
    }
    struct Fixture {
        owner: OrdinaryOwner,
        context: AgentContext,
        proposal_sha256: [u8; 32],
        intent_sha256: [u8; 32],
    }
    impl Fixture {
        fn new() -> Self {
            let temp = tempfile::tempdir().unwrap();
            let mut runtime = HostRuntime::new(
                Store::open(&temp.path().join("ordinary.sqlite"), EventBudget::default()).unwrap(),
            )
            .unwrap();
            let host = Arc::new(SessionExecHost::new(&mut runtime).unwrap());
            let connection = Arc::new(runtime.connect().unwrap());
            let rights = Capabilities {
                session_read: true,
                session_write: true,
                propose: true,
                execute: true,
                retire: false,
            };
            let admission = host
                .admit(
                    &runtime,
                    &connection,
                    rights,
                    rights,
                    vec!["synthetic-session".into()],
                    "synthetic-domain".into(),
                    1000,
                    0,
                )
                .unwrap();
            let create = Request::new_for_generation(
                "trusted-create",
                host.generation(&runtime).unwrap(),
                Action::Create {
                    session_id: "synthetic-session".into(),
                    parent: None,
                    parent_tail: 0,
                },
            )
            .unwrap();
            let raw = host
                .dispatch(&mut runtime, &connection, &admission, create.raw(), || 1)
                .unwrap();
            assert!(matches!(
                Reply::decode_for(&create, &raw).unwrap().outcome,
                Outcome::Session(_)
            ));
            // Ordinary logical coordination only: this synthetic artifact is
            // never launched, and these tests do not qualify native/sandbox IO.
            let artifact = temp.path().join("never-executed-artifact.bin");
            std::fs::write(&artifact, b"ordinary-fixed-bytes").unwrap();
            let proposal = Request::new_for_generation(
                "trusted-propose",
                host.generation(&runtime).unwrap(),
                Action::Propose {
                    session_id: "synthetic-session".into(),
                    intent: Intent {
                        operation_id: OPERATION.into(),
                        artifact_sha256: hash(b"ordinary-fixed-bytes"),
                        program: artifact.to_str().unwrap().into(),
                        argv: vec!["synthetic-argument".into()],
                        cwd: temp.path().to_str().unwrap().into(),
                        env: vec![Environment {
                            name: "MORROW_SYNTHETIC".into(),
                            value: "ordinary-value".into(),
                        }],
                        input: b"ordinary-input".to_vec(),
                        execution_domain: "synthetic-domain".into(),
                        max_runtime_ms: 100,
                    },
                },
            )
            .unwrap();
            let raw = host
                .dispatch(&mut runtime, &connection, &admission, proposal.raw(), || 2)
                .unwrap();
            assert!(matches!(
                Reply::decode_for(&proposal, &raw).unwrap().outcome,
                Outcome::Tool(info) if info.phase == ToolPhase::Proposed
            ));
            let review = SensitiveReview(Some(host.review_tool(&runtime, OPERATION, 3).unwrap()));
            let resources = BorrowedNativeResources::for_owner(&runtime).unwrap();
            let context = AgentContext::new(
                host,
                ProcessHost::default(),
                resources,
                None,
                connection,
                admission,
            )
            .unwrap_or_else(|_| panic!("ordinary context rejected"));
            Self {
                owner: OrdinaryOwner {
                    runtime,
                    revoke_on_finish: None,
                    fail_finish: false,
                    finishes: 0,
                    _temp: temp,
                },
                context,
                proposal_sha256: review.get().proposal_sha256,
                intent_sha256: review.get().intent_sha256,
            }
        }
        fn observation(&self) -> ToolObservation {
            self.context
                .host
                .inspect_tool_record(&self.owner.runtime, OPERATION)
                .unwrap()
        }
        fn claim(&mut self, expected: &ToolObservation) -> Result<AgentReply, AgentError> {
            approve_claim(
                &mut self.owner,
                &self.context,
                "trusted-claim",
                OPERATION,
                self.proposal_sha256,
                self.intent_sha256,
                expected.record_revision,
                expected.record_sha256,
                &|| 4,
            )
        }
    }

    #[test]
    fn ordinary_review_maintenance_revocation_prevents_sensitive_delivery() {
        let mut fixture = Fixture::new();
        fixture.owner.revoke_on_finish = Some(
            fixture
                .owner
                .runtime
                .revocation(&fixture.context.executor_connection)
                .unwrap(),
        );
        assert!(matches!(
            review(&mut fixture.owner, &fixture.context, OPERATION, &|| 4),
            Err(AgentError::Invalid)
        ));
        assert_eq!(fixture.owner.finishes, 1);
        assert_eq!(fixture.observation().phase, ToolPhase::Proposed);
    }
    #[test]
    fn ordinary_claim_maintenance_revocation_preserves_consumed_unknown() {
        let mut fixture = Fixture::new();
        let before = fixture.observation();
        fixture.owner.revoke_on_finish = Some(
            fixture
                .owner
                .runtime
                .revocation(&fixture.context.executor_connection)
                .unwrap(),
        );
        assert!(matches!(fixture.claim(&before), Err(AgentError::Unknown)));
        let after = fixture.observation();
        assert_eq!(after.identity, before.identity);
        assert_eq!(after.phase, ToolPhase::DispatchUnknown);
        assert!(!after.invocation_started);
        assert!(after.record_revision > before.record_revision);
        // A fresh command is not a replay mechanism; the consumed marker remains.
        assert!(fixture.claim(&before).is_err());
        assert_eq!(fixture.observation(), after);
        assert_eq!(fixture.owner.finishes, 1);
    }
    #[test]
    fn ordinary_stale_revision_or_whole_record_hash_cannot_approve() {
        let mut fixture = Fixture::new();
        let before = fixture.observation();
        let mut stale = before.clone();
        stale.record_revision = before.record_revision.saturating_add(1);
        assert!(matches!(fixture.claim(&stale), Err(AgentError::Invalid)));
        let mut wrong_hash = before.clone();
        wrong_hash.record_sha256[0] ^= 1;
        assert!(matches!(
            fixture.claim(&wrong_hash),
            Err(AgentError::Invalid)
        ));
        assert_eq!(fixture.observation(), before);
        assert_eq!(fixture.owner.finishes, 0);
    }
    #[test]
    fn ordinary_happy_claim_is_exactly_once_and_keeps_original_binding() {
        let mut fixture = Fixture::new();
        let binding = fixture.owner.runtime.binding();
        let original_connection = fixture.context.executor_connection.clone();
        let before = fixture.observation();
        let response = fixture.claim(&before).unwrap();
        let after = fixture.observation();
        match &response {
            AgentReply::Claimed { observation, claim } => {
                assert_eq!(observation, &after);
                assert_ne!(*claim, [0; 32]);
            }
            _ => panic!("expected one correlated original claim"),
        }
        assert_eq!(after.identity, before.identity);
        assert_eq!(after.phase, ToolPhase::DispatchUnknown);
        assert!(!after.invocation_started);
        assert_eq!(fixture.owner.runtime.binding(), binding);
        assert!(Arc::ptr_eq(
            &original_connection,
            &fixture.context.executor_connection
        ));
        assert_eq!(fixture.owner.finishes, 1);
        assert!(matches!(fixture.claim(&before), Err(AgentError::Invalid)));
        assert_eq!(fixture.observation(), after);
    }
    #[test]
    fn ordinary_wrong_session_admission_cannot_review_or_approve() {
        let mut fixture = Fixture::new();
        let before = fixture.observation();
        let rights = Capabilities {
            session_read: true,
            execute: true,
            ..Default::default()
        };
        let wrong: Admission = fixture
            .context
            .host
            .admit(
                &fixture.owner.runtime,
                &fixture.context.executor_connection,
                rights,
                rights,
                vec!["different-session".into()],
                "synthetic-domain".into(),
                1000,
                3,
            )
            .unwrap();
        fixture.context.executor_admission = wrong;
        assert!(matches!(
            review(&mut fixture.owner, &fixture.context, OPERATION, &|| 4),
            Err(AgentError::Invalid)
        ));
        assert!(matches!(fixture.claim(&before), Err(AgentError::Invalid)));
        assert_eq!(fixture.observation(), before);
        assert_eq!(fixture.owner.finishes, 0);
    }
    #[test]
    fn ordinary_maintenance_failure_never_delivers_consumed_claim() {
        let mut fixture = Fixture::new();
        fixture.owner.fail_finish = true;
        let before = fixture.observation();
        assert!(matches!(fixture.claim(&before), Err(AgentError::Unknown)));
        let after = fixture.observation();
        assert_eq!(after.phase, ToolPhase::DispatchUnknown);
        assert!(!after.invocation_started);
        assert!(matches!(fixture.claim(&before), Err(AgentError::Invalid)));
        assert_eq!(fixture.observation(), after);
        assert_eq!(fixture.owner.finishes, 1);
    }
}
