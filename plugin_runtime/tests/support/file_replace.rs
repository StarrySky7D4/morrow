//! Original managed conditional replacement: immutable claim, exact outcome, and no replay.
use super::*;
use morrow_core::{
    file_mutation::{Disposition, MutationRequest, RequestRecord, Target},
    io_evidence::Kind,
    io_intent::Phase,
};
use morrow_plugin_runtime::{
    file_target::{Error as ReplaceError, SelectionScope, TargetBroker},
    io_binding::Error as AdmissionError,
};
use sha2::{Digest, Sha256};
use std::{fs, path::PathBuf};

const SUBJECT: &str = "plugin.file-replace-test";
const OP: &str = "replace-selected-once";
const APPROVAL: [u8; 32] = [0x61; 32];
const ORIGINAL: &[u8] = b"original retained target bytes";
const REPLACEMENT: &[u8] = b"staged replacement only";

fn setup(
    stage: bool,
) -> (
    Fixture,
    ManagedInstance,
    IoBinding,
    TargetBroker,
    RequestRecord,
    PathBuf,
    PathBuf,
) {
    let caps = BTreeSet::from([IoCapability::FileReplace]);
    let mut f = Fixture::with_capabilities(true, 8, 100_000, caps.clone());
    let instance = f.connect();
    let binding = f
        .manager
        .bind_io(
            &f.host,
            &instance,
            f.package.digest(),
            f.manager.revision(),
            &caps,
            90,
            1,
        )
        .unwrap();
    let target = f._dir.path().join("replace-target.bin");
    let alias = f._dir.path().join("replace-alias.bin");
    fs::write(&target, ORIGINAL).unwrap();
    fs::hard_link(&target, &alias).unwrap();
    let mut broker = TargetBroker::new([0x62; 32]).unwrap();
    let selected = broker
        .select_existing(
            &f.manager,
            &f.host,
            &instance,
            &binding,
            &target,
            SelectionScope {
                subject: SUBJECT.into(),
                approval_sha256: APPROVAL,
                disposition: Disposition::Replace,
            },
            || 2,
        )
        .unwrap();
    let request = RequestRecord::new(MutationRequest {
        operation_id: OP.into(),
        subject: SUBJECT.into(),
        package_sha256: f.package.digest(),
        approval_sha256: APPROVAL,
        target: Target {
            reference: selected.reference,
            relative_path: None,
        },
        disposition: Disposition::Replace,
        expected_identity: Some(selected.expected_identity),
        content_length: REPLACEMENT.len() as u64,
        content_sha256: Some(Sha256::digest(REPLACEMENT).into()),
    })
    .unwrap();
    broker
        .prepare_request(&f.manager, &mut f.host, &instance, &request, || 3)
        .unwrap();
    if stage {
        broker
            .stage_content(
                &f.manager,
                &mut f.host,
                &instance,
                &request,
                REPLACEMENT,
                || 4,
            )
            .unwrap();
    }
    (f, instance, binding, broker, request, target, alias)
}

fn prepared(f: &Fixture, request: &RequestRecord, staged: bool) {
    let store = f.host.store_local();
    assert_eq!(
        store
            .lookup_io_intent(SUBJECT, OP)
            .unwrap()
            .unwrap()
            .phase(),
        Phase::Prepared
    );
    assert_eq!(
        store
            .io_material(SUBJECT, OP, Kind::Request)
            .unwrap()
            .unwrap()
            .payload(),
        request.container()
    );
    assert!(matches!(
        store.io_material(SUBJECT, OP, Kind::Response),
        Err(morrow_core::Error::EvidenceUnavailable)
    ));
    let content = store
        .file_mutation_content_local_authorized(SUBJECT, OP, || Ok(()))
        .unwrap();
    assert_eq!(content.is_some(), staged);
    if let Some(content) = content {
        assert_eq!(content.content(), REPLACEMENT);
    }
    assert_eq!(
        store.pending(0, 10).unwrap().len(),
        if staged { 2 } else { 1 }
    );
    store.integrity_check().unwrap();
}

#[test]
fn real_selected_replace_records_exact_outcome_and_repeated_calls_cannot_dispatch_again() {
    let (mut f, instance, binding, mut broker, request, target, alias) = setup(true);
    let before = binding.usage();
    let outcome = broker
        .replace(&f.manager, &mut f.host, &instance, &request, || 5)
        .unwrap();
    assert_eq!(
        outcome.result(),
        morrow_core::file_effect::ReplaceResult::Replaced
    );
    assert_eq!(fs::read(&target).unwrap(), REPLACEMENT);
    assert_eq!(fs::read(&alias).unwrap(), ORIGINAL);
    assert_eq!(binding.usage().resources, 0);
    assert_eq!(binding.usage().jobs, 0);
    let command = request.command().unwrap();
    assert_eq!(
        binding.usage().bytes - before.bytes,
        command.request_bytes + command.response_limit + REPLACEMENT.len() as u64
    );
    assert_eq!(
        f.host
            .store_local()
            .lookup_io_intent(SUBJECT, OP)
            .unwrap()
            .unwrap()
            .phase(),
        Phase::Observed
    );
    assert_eq!(
        f.host
            .store_local()
            .io_material(SUBJECT, OP, Kind::Response)
            .unwrap()
            .unwrap()
            .payload(),
        outcome.container()
    );
    let usage = binding.usage();
    let pending = f.host.store_local().pending(0, 10).unwrap();
    for tick in [6, 7, 8] {
        assert!(matches!(
            broker.replace(&f.manager, &mut f.host, &instance, &request, || tick),
            Err(ReplaceError::AlreadyDispatched)
        ));
        assert_eq!(binding.usage(), usage);
        assert_eq!(f.host.store_local().pending(0, 10).unwrap(), pending);
        assert_eq!(fs::read(&target).unwrap(), REPLACEMENT);
    }
    assert!(!fs::read_dir(f._dir.path()).unwrap().any(|entry| {
        entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with(".morrow-replace-")
    }));
    assert_eq!(broker.len(), 0);
    f.host.store_local().integrity_check().unwrap();
    instance.close(&mut f.host).unwrap();
}

#[test]
fn missing_durable_content_keeps_prepared_and_charges_only_the_attempted_execution() {
    let (mut f, instance, binding, mut broker, request, target, alias) = setup(false);
    let usage = binding.usage();
    assert!(matches!(
        broker.replace(&f.manager, &mut f.host, &instance, &request, || 4),
        Err(ReplaceError::Persistence(
            morrow_core::Error::EvidenceUnavailable
        ))
    ));
    assert_eq!(binding.usage().resources, usage.resources);
    assert_eq!(binding.usage().jobs, usage.jobs);
    let command = request.command().unwrap();
    assert_eq!(
        binding.usage().bytes - usage.bytes,
        command.request_bytes + command.response_limit + REPLACEMENT.len() as u64
    );
    prepared(&f, &request, false);
    broker
        .release(
            &f.manager,
            &f.host,
            &instance,
            request.request().target.reference,
            5,
        )
        .unwrap();
    assert_eq!(binding.usage().resources, 0);
    assert_eq!(fs::read(&target).unwrap(), ORIGINAL);
    assert_eq!(fs::read(&alias).unwrap(), ORIGINAL);
    instance.close(&mut f.host).unwrap();
}

#[test]
fn foreign_owner_or_wrong_plan_is_rejected_before_dispatch_or_budget_charge() {
    let (mut f, instance, binding, mut broker, request, target, alias) = setup(true);
    let foreign = f.connect();
    let usage = binding.usage();
    assert!(matches!(
        broker.replace(&f.manager, &mut f.host, &foreign, &request, || 5),
        Err(ReplaceError::Admission(AdmissionError::Denied))
    ));
    for variant in [
        "subject",
        "package",
        "approval",
        "identity",
        "reference",
        "action",
    ] {
        let mut changed = request.request().clone();
        match variant {
            "subject" => changed.subject = "foreign-subject".into(),
            "package" => changed.package_sha256 = [9; 32],
            "approval" => changed.approval_sha256 = [8; 32],
            "identity" => changed.expected_identity = Some([7; 32]),
            "reference" => changed.target.reference = [6; 32],
            "action" => {
                changed.disposition = Disposition::Delete;
                changed.content_length = 0;
                changed.content_sha256 = None;
            }
            _ => unreachable!(),
        }
        let changed = RequestRecord::new(changed).unwrap();
        let expected = if variant == "reference" {
            ReplaceError::Missing
        } else {
            ReplaceError::Mismatch
        };
        assert!(
            matches!(
                broker.replace(&f.manager, &mut f.host, &instance, &changed, || 5),
                Err(error) if error == expected
            ),
            "{variant}"
        );
    }
    assert_eq!(binding.usage(), usage);
    prepared(&f, &request, true);
    foreign.close(&mut f.host).unwrap();
    broker
        .release(
            &f.manager,
            &f.host,
            &instance,
            request.request().target.reference,
            6,
        )
        .unwrap();
    assert_eq!(fs::read(&target).unwrap(), ORIGINAL);
    assert_eq!(fs::read(&alias).unwrap(), ORIGINAL);
    instance.close(&mut f.host).unwrap();
}
