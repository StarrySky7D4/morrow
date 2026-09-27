//! Real Windows FileCreate effects, confined to each test's TempDir.
use super::*;
use morrow_core::{
    file_effect::{CreateOutcome, CreateResult},
    file_mutation::{Disposition, MutationRequest, RequestRecord, Target},
    file_path::RelativeFilePath,
    io_evidence::Kind,
    io_intent::Phase,
};
use morrow_plugin_runtime::{
    file_target::{Error as CreateError, SelectionScope, TargetBroker},
    io_binding::Error as AdmissionError,
};
use sha2::{Digest, Sha256};
use std::{fs, path::PathBuf};

const SUBJECT: &str = "plugin.file-create-test";
const OP: &str = "create-selected-once";
const APPROVAL: [u8; 32] = [0xa7; 32];
const CONTENT: &[u8] = b"created only inside test tempdir";

fn serial_effects() -> std::sync::MutexGuard<'static, ()> {
    super::file_delete::serial_effects()
}

fn create_caps() -> BTreeSet<IoCapability> {
    BTreeSet::from([IoCapability::FileCreate])
}

pub(super) fn setup(
    bytes: &[u8],
    relative: &str,
    resources: u32,
) -> (
    Fixture,
    ManagedInstance,
    IoBinding,
    TargetBroker,
    RequestRecord,
    PathBuf,
) {
    let budget = 100_000_u64.max((bytes.len() as u64).saturating_mul(3));
    let mut f = Fixture::with_capabilities(true, resources, budget, create_caps());
    let instance = f.connect();
    let binding = f
        .manager
        .bind_io(
            &f.host,
            &instance,
            f.package.digest(),
            f.manager.revision(),
            &create_caps(),
            90,
            1,
        )
        .unwrap();
    let root = f._dir.path().join("create-root");
    fs::create_dir(&root).unwrap();
    let relative = RelativeFilePath::parse(relative).unwrap();
    let segments: Vec<_> = relative.as_str().split('/').collect();
    let mut parent = root.clone();
    for segment in &segments[..segments.len() - 1] {
        parent.push(segment);
        fs::create_dir(&parent).unwrap();
    }
    let leaf = parent.join(segments.last().unwrap());
    let mut broker = TargetBroker::new([0xa8; 32]).unwrap();
    let selected = broker
        .select_create(
            &f.manager,
            &f.host,
            &instance,
            &binding,
            &root,
            &relative,
            SelectionScope {
                subject: SUBJECT.into(),
                approval_sha256: APPROVAL,
                disposition: Disposition::Create,
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
            relative_path: Some(relative),
        },
        disposition: Disposition::Create,
        expected_identity: None,
        content_length: bytes.len() as u64,
        content_sha256: Some(Sha256::digest(bytes).into()),
    })
    .unwrap();
    broker
        .prepare_request(&f.manager, &mut f.host, &instance, &request, || 3)
        .unwrap();
    (f, instance, binding, broker, request, leaf)
}

pub(super) fn stage(
    f: &mut Fixture,
    instance: &ManagedInstance,
    broker: &TargetBroker,
    request: &RequestRecord,
    bytes: &[u8],
) {
    broker
        .stage_content(&f.manager, &mut f.host, instance, request, bytes, || 4)
        .unwrap();
}

pub(super) fn phase(f: &Fixture) -> Phase {
    f.host
        .store_local()
        .lookup_io_intent(SUBJECT, OP)
        .unwrap()
        .unwrap()
        .phase()
}

pub(super) fn observed(f: &Fixture, request: &RequestRecord) -> CreateOutcome {
    assert_eq!(phase(f), Phase::Observed);
    let material = f
        .host
        .store_local()
        .io_material(SUBJECT, OP, Kind::Response)
        .unwrap()
        .unwrap();
    let outcome = CreateOutcome::decode(material.payload()).unwrap();
    assert_eq!(outcome.operation_id(), OP);
    assert_eq!(outcome.subject(), SUBJECT);
    assert_eq!(
        outcome.request_sha256(),
        request.command().unwrap().request_sha256
    );
    assert_eq!(
        outcome.target_reference(),
        request.request().target.reference
    );
    f.host.store_local().integrity_check().unwrap();
    outcome
}

#[test]
fn creates_nested_unicode_file_and_observes_exact_original_response() {
    let _serial = serial_effects();
    let (mut f, instance, binding, mut broker, request, leaf) =
        setup(CONTENT, "子目录/新建.txt", 4);
    assert!(!leaf.exists());
    stage(&mut f, &instance, &broker, &request, CONTENT);
    let before = f.host.store_local().pending(0, 10).unwrap();
    assert_eq!(before.len(), 2);
    let result = broker
        .create(&f.manager, &mut f.host, &instance, &request, || 5)
        .unwrap();
    assert_eq!(result.result(), CreateResult::Created);
    assert_eq!(fs::read(&leaf).unwrap(), CONTENT);
    assert_eq!(observed(&f, &request).container(), result.container());
    assert_eq!(f.host.store_local().pending(0, 10).unwrap().len(), 4);
    assert_eq!(binding.usage().resources, 0);
    instance.close(&mut f.host).unwrap();
}

#[test]
fn empty_content_creates_empty_file_with_valid_observed_digest() {
    let _serial = serial_effects();
    let (mut f, instance, _binding, mut broker, request, leaf) = setup(b"", "empty.bin", 2);
    stage(&mut f, &instance, &broker, &request, b"");
    let result = broker
        .create(&f.manager, &mut f.host, &instance, &request, || 5)
        .unwrap();
    assert_eq!(result.result(), CreateResult::Created);
    assert_eq!(fs::metadata(&leaf).unwrap().len(), 0);
    assert_eq!(observed(&f, &request).container(), result.container());
    instance.close(&mut f.host).unwrap();
}

#[test]
fn existing_file_or_directory_rejects_without_modifying_leaf_or_leaving_temp() {
    let _serial = serial_effects();
    for directory in [false, true] {
        let (mut f, instance, _binding, mut broker, request, leaf) = setup(CONTENT, "occupied", 2);
        if directory {
            fs::create_dir(&leaf).unwrap();
        } else {
            fs::write(&leaf, b"preexisting bytes").unwrap();
        }
        stage(&mut f, &instance, &broker, &request, CONTENT);
        let result = broker
            .create(&f.manager, &mut f.host, &instance, &request, || 5)
            .unwrap();
        assert!(matches!(
            result.result(),
            CreateResult::OsRejected { code: 1.. }
        ));
        assert_eq!(observed(&f, &request).container(), result.container());
        if directory {
            assert!(leaf.is_dir());
        } else {
            assert_eq!(fs::read(&leaf).unwrap(), b"preexisting bytes");
        }
        let leftovers: Vec<_> = fs::read_dir(leaf.parent().unwrap())
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .filter(|name| name.starts_with(".morrow-create-") && name.ends_with(".tmp"))
            .collect();
        assert!(
            leftovers.is_empty(),
            "temporary files left after OS rejection: {leftovers:?}"
        );
        instance.close(&mut f.host).unwrap();
    }
}

#[test]
fn occupied_temporary_name_never_modifies_an_existing_file_directory_or_hardlink() {
    let _serial = serial_effects();
    for occupant in ["file", "directory", "hardlink"] {
        let (mut f, instance, _binding, mut broker, request, leaf) =
            setup(CONTENT, "destination.bin", 2);
        stage(&mut f, &instance, &broker, &request, CONTENT);
        let digest = request.command().unwrap().request_sha256;
        let temporary = leaf.parent().unwrap().join(format!(
            ".morrow-create-{}.tmp",
            digest
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect::<String>()
        ));
        let linked_source = f._dir.path().join("external-linked-source.bin");
        match occupant {
            "file" => fs::write(&temporary, b"preexisting temporary file").unwrap(),
            "directory" => fs::create_dir(&temporary).unwrap(),
            "hardlink" => {
                fs::write(&temporary, b"linked original must survive").unwrap();
                fs::hard_link(&temporary, &linked_source).unwrap();
            }
            _ => unreachable!(),
        }
        let result = broker
            .create(&f.manager, &mut f.host, &instance, &request, || 5)
            .unwrap();
        assert!(matches!(
            result.result(),
            CreateResult::OsRejected { code: 1.. }
        ));
        assert_eq!(observed(&f, &request).container(), result.container());
        assert!(!leaf.exists(), "{occupant}");
        match occupant {
            "file" => assert_eq!(fs::read(&temporary).unwrap(), b"preexisting temporary file"),
            "directory" => assert!(temporary.is_dir()),
            "hardlink" => {
                assert_eq!(
                    fs::read(&temporary).unwrap(),
                    b"linked original must survive"
                );
                assert_eq!(
                    fs::read(&linked_source).unwrap(),
                    b"linked original must survive"
                );
            }
            _ => unreachable!(),
        }
        instance.close(&mut f.host).unwrap();
    }
}

#[test]
fn missing_staged_content_or_temp_resource_does_not_claim_or_create() {
    let _serial = serial_effects();
    for missing_stage in [true, false] {
        // One root resource is enough for selection, but not the effect temp slot.
        let resources = if missing_stage { 2 } else { 1 };
        let (mut f, instance, binding, mut broker, request, leaf) =
            setup(CONTENT, "new.bin", resources);
        if !missing_stage {
            stage(&mut f, &instance, &broker, &request, CONTENT);
        }
        let prior = f.host.store_local().pending(0, 10).unwrap();
        let result = broker.create(&f.manager, &mut f.host, &instance, &request, || 5);
        if missing_stage {
            assert!(result.is_err());
        } else {
            assert!(matches!(
                result,
                Err(CreateError::Admission(AdmissionError::Limit))
            ));
        }
        assert_eq!(phase(&f), Phase::Prepared);
        assert_eq!(f.host.store_local().pending(0, 10).unwrap(), prior);
        assert!(!leaf.exists());
        assert_eq!(binding.usage().resources, 1);
        instance.close(&mut f.host).unwrap();
    }
}

#[test]
fn foreign_owner_and_changed_plan_cannot_create() {
    let _serial = serial_effects();
    let (mut f, instance, _binding, mut broker, request, leaf) = setup(CONTENT, "new.bin", 2);
    stage(&mut f, &instance, &broker, &request, CONTENT);
    let foreign = f.connect();
    assert!(matches!(
        broker.create(&f.manager, &mut f.host, &foreign, &request, || 5),
        Err(CreateError::Admission(AdmissionError::Denied))
    ));
    for variant in [
        "subject",
        "package",
        "approval",
        "relative",
        "reference",
        "action",
    ] {
        let mut changed = request.request().clone();
        match variant {
            "subject" => changed.subject = "another-subject".into(),
            "package" => changed.package_sha256 = [5; 32],
            "approval" => changed.approval_sha256 = [6; 32],
            "relative" => {
                changed.target.relative_path =
                    Some(RelativeFilePath::parse("different.bin").unwrap())
            }
            "reference" => changed.target.reference = [7; 32],
            "action" => {
                changed.disposition = Disposition::Replace;
                changed.expected_identity = Some([8; 32]);
            }
            _ => unreachable!(),
        }
        let changed = RequestRecord::new(changed).unwrap();
        assert!(
            broker
                .create(&f.manager, &mut f.host, &instance, &changed, || 5)
                .is_err(),
            "{variant}"
        );
        assert_eq!(phase(&f), Phase::Prepared);
    }
    assert!(!leaf.exists());
    foreign.close(&mut f.host).unwrap();
    instance.close(&mut f.host).unwrap();
}

#[test]
fn repeated_operation_cannot_recreate_a_later_entry_at_the_same_name() {
    let _serial = serial_effects();
    let (mut f, instance, _binding, mut broker, request, leaf) = setup(CONTENT, "once.bin", 2);
    stage(&mut f, &instance, &broker, &request, CONTENT);
    assert_eq!(
        broker
            .create(&f.manager, &mut f.host, &instance, &request, || 5)
            .unwrap()
            .result(),
        CreateResult::Created
    );
    fs::remove_file(&leaf).unwrap();
    fs::write(&leaf, b"later unrelated entry").unwrap();
    assert!(matches!(
        broker.create(&f.manager, &mut f.host, &instance, &request, || 6),
        Err(CreateError::AlreadyDispatched)
    ));
    assert_eq!(fs::read(&leaf).unwrap(), b"later unrelated entry");
    assert_eq!(observed(&f, &request).result(), CreateResult::Created);
    instance.close(&mut f.host).unwrap();
}

#[test]
fn broker_drop_loses_selection_but_historical_result_remains_readable() {
    let _serial = serial_effects();
    let (mut f, instance, _binding, mut broker, request, leaf) = setup(CONTENT, "persisted.bin", 2);
    stage(&mut f, &instance, &broker, &request, CONTENT);
    broker
        .create(&f.manager, &mut f.host, &instance, &request, || 5)
        .unwrap();
    drop(broker);
    let mut fresh = TargetBroker::new([0xa8; 32]).unwrap();
    assert!(matches!(
        fresh.create(&f.manager, &mut f.host, &instance, &request, || 6),
        Err(CreateError::Missing)
    ));
    assert_eq!(fs::read(&leaf).unwrap(), CONTENT);
    assert_eq!(observed(&f, &request).result(), CreateResult::Created);
    instance.close(&mut f.host).unwrap();
}

fn successful_create_samples() -> usize {
    let (mut f, instance, _binding, mut broker, request, leaf) =
        setup(CONTENT, "clock-probe.bin", 2);
    stage(&mut f, &instance, &broker, &request, CONTENT);
    let mut samples = 0;
    let outcome = broker
        .create(&f.manager, &mut f.host, &instance, &request, || {
            samples += 1;
            5
        })
        .unwrap();
    assert_eq!(outcome.result(), CreateResult::Created);
    assert_eq!(fs::read(&leaf).unwrap(), CONTENT);
    assert!(samples >= 10);
    samples
}

#[test]
fn preclaim_stop_or_expiry_leaves_prepared_and_no_file() {
    let _serial = serial_effects();
    for revoke in ["stop", "expiry"] {
        let (mut f, instance, _binding, mut broker, request, leaf) =
            setup(CONTENT, "preclaim.bin", 2);
        stage(&mut f, &instance, &broker, &request, CONTENT);
        let prior = f.host.store_local().pending(0, 10).unwrap();
        let mut ticks = 0;
        let result = broker.create(&f.manager, &mut f.host, &instance, &request, || {
            ticks += 1;
            if ticks == 1 {
                if revoke == "stop" {
                    instance.stop();
                } else {
                    return 90;
                }
            }
            5
        });
        assert!(matches!(result, Err(CreateError::Admission(_))), "{revoke}");
        assert_eq!(phase(&f), Phase::Prepared);
        assert_eq!(f.host.store_local().pending(0, 10).unwrap(), prior);
        assert!(!leaf.exists());
        instance.close(&mut f.host).unwrap();
    }
}

#[test]
fn after_claim_revocation_is_unknown_and_never_replays_temp_or_leaf() {
    let _serial = serial_effects();
    let samples = successful_create_samples();
    // For a one-chunk payload: final delivery, post-effect sample, four native
    // liveness checks. The first native check is after the committed claim.
    let after_claim = samples - 5;
    for revoke in ["stop", "expiry"] {
        let (mut f, instance, _binding, mut broker, request, leaf) =
            setup(CONTENT, "unknown.bin", 2);
        stage(&mut f, &instance, &broker, &request, CONTENT);
        let mut ticks = 0;
        let result = broker.create(&f.manager, &mut f.host, &instance, &request, || {
            ticks += 1;
            if ticks == after_claim {
                if revoke == "stop" {
                    instance.stop();
                } else {
                    return 90;
                }
            }
            5
        });
        assert!(
            matches!(result, Err(CreateError::OutcomeUnknown)),
            "{revoke}"
        );
        assert_eq!(phase(&f), Phase::OutcomeUnknown);
        assert!(!leaf.exists());
        assert!(matches!(
            broker.create(&f.manager, &mut f.host, &instance, &request, || 90),
            Err(CreateError::Admission(_))
        ));
        f.host.store_local().integrity_check().unwrap();
        instance.close(&mut f.host).unwrap();
    }
}

#[test]
fn loss_before_publish_cleans_temp_but_keeps_unknown() {
    let _serial = serial_effects();
    let samples = successful_create_samples();
    let before_publish = samples - 2;
    let (mut f, instance, _binding, mut broker, request, leaf) =
        setup(CONTENT, "never-publish.bin", 2);
    stage(&mut f, &instance, &broker, &request, CONTENT);
    let mut ticks = 0;
    let result = broker.create(&f.manager, &mut f.host, &instance, &request, || {
        ticks += 1;
        if ticks == before_publish {
            instance.stop();
        }
        5
    });
    assert!(matches!(result, Err(CreateError::OutcomeUnknown)));
    assert_eq!(phase(&f), Phase::OutcomeUnknown);
    assert!(!leaf.exists());
    let leftovers: Vec<_> = fs::read_dir(leaf.parent().unwrap())
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .filter(|name| name.starts_with(".morrow-create-") && name.ends_with(".tmp"))
        .collect();
    assert!(
        leftovers.is_empty(),
        "temporary files left after cleanup: {leftovers:?}"
    );
    instance.close(&mut f.host).unwrap();
}

#[test]
fn post_effect_or_post_observe_revocation_retains_created_observation() {
    let _serial = serial_effects();
    let samples = successful_create_samples();
    for revoke_at in [samples - 1, samples] {
        let (mut f, instance, _binding, mut broker, request, leaf) =
            setup(CONTENT, "observed.bin", 2);
        stage(&mut f, &instance, &broker, &request, CONTENT);
        let mut ticks = 0;
        let result = broker.create(&f.manager, &mut f.host, &instance, &request, || {
            ticks += 1;
            if ticks == revoke_at {
                instance.stop();
            }
            5
        });
        assert!(matches!(
            result,
            Err(CreateError::CommittedButDeliveryDenied(
                AdmissionError::Denied
            ))
        ));
        assert_eq!(fs::read(&leaf).unwrap(), CONTENT);
        assert_eq!(observed(&f, &request).result(), CreateResult::Created);
        instance.close(&mut f.host).unwrap();
    }
}

#[test]
fn another_broker_is_busy_before_claim_and_can_create_after_gate_releases() {
    let _serial = serial_effects();
    let (mut first, first_instance, _first_binding, mut first_broker, first_request, first_leaf) =
        setup(CONTENT, "first.bin", 2);
    let (
        mut second,
        second_instance,
        _second_binding,
        mut second_broker,
        second_request,
        second_leaf,
    ) = setup(CONTENT, "second.bin", 2);
    stage(
        &mut first,
        &first_instance,
        &first_broker,
        &first_request,
        CONTENT,
    );
    stage(
        &mut second,
        &second_instance,
        &second_broker,
        &second_request,
        CONTENT,
    );
    let mut ticks = 0;
    let mut attempted = false;
    let first_outcome = first_broker
        .create(
            &first.manager,
            &mut first.host,
            &first_instance,
            &first_request,
            || {
                ticks += 1;
                // Gate is acquired after validation and durable Prepared read, before
                // the temporary resource admission.
                if ticks == 4 {
                    attempted = true;
                    assert!(matches!(
                        second_broker.create(
                            &second.manager,
                            &mut second.host,
                            &second_instance,
                            &second_request,
                            || 5
                        ),
                        Err(CreateError::Busy)
                    ));
                    assert_eq!(phase(&second), Phase::Prepared);
                    assert!(!second_leaf.exists());
                }
                5
            },
        )
        .unwrap();
    assert!(attempted);
    assert_eq!(first_outcome.result(), CreateResult::Created);
    assert_eq!(fs::read(&first_leaf).unwrap(), CONTENT);
    assert_eq!(phase(&second), Phase::Prepared);
    let second_outcome = second_broker
        .create(
            &second.manager,
            &mut second.host,
            &second_instance,
            &second_request,
            || 6,
        )
        .unwrap();
    assert_eq!(second_outcome.result(), CreateResult::Created);
    assert_eq!(fs::read(&second_leaf).unwrap(), CONTENT);
    first_instance.close(&mut first.host).unwrap();
    second_instance.close(&mut second.host).unwrap();
}
