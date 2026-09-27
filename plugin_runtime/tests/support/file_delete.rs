//! Windows FileDelete integration against files created only in each test's TempDir.
use super::*;
use morrow_core::{
    file_effect::{DeleteOutcome, DeleteResult},
    file_mutation::{Disposition, MutationRequest, RequestRecord, Target},
    io_evidence::Kind,
    io_intent::Phase,
};
use morrow_plugin_runtime::{
    file_target::{Error as DeleteError, SelectionScope, TargetBroker},
    io_binding::Error as AdmissionError,
};
use std::{
    fs,
    path::PathBuf,
    sync::{Mutex, MutexGuard, OnceLock},
};

pub(super) fn serial_effects() -> MutexGuard<'static, ()> {
    static SERIAL: OnceLock<Mutex<()>> = OnceLock::new();
    SERIAL
        .get_or_init(|| Mutex::new(()))
        .lock()
        .unwrap_or_else(|error| error.into_inner())
}

const DELETE_SUBJECT: &str = "plugin.delete-selected-test";
const DELETE_APPROVAL: [u8; 32] = [0xd1; 32];
const DELETE_OP: &str = "delete-selected-once";

fn delete_caps() -> BTreeSet<IoCapability> {
    BTreeSet::from([IoCapability::FileDelete])
}

fn delete_request(
    f: &Fixture,
    selected: morrow_plugin_runtime::file_target::SelectedTarget,
) -> RequestRecord {
    RequestRecord::new(MutationRequest {
        operation_id: DELETE_OP.into(),
        subject: DELETE_SUBJECT.into(),
        package_sha256: f.package.digest(),
        approval_sha256: DELETE_APPROVAL,
        target: Target {
            reference: selected.reference,
            relative_path: None,
        },
        disposition: Disposition::Delete,
        expected_identity: Some(selected.expected_identity),
        content_length: 0,
        content_sha256: None,
    })
    .unwrap()
}

fn setup_with_readonly(
    readonly: bool,
) -> (
    Fixture,
    ManagedInstance,
    IoBinding,
    TargetBroker,
    RequestRecord,
    PathBuf,
) {
    let mut f = Fixture::with_capabilities(true, 2, 100_000, delete_caps());
    let instance = f.connect();
    let binding = f
        .manager
        .bind_io(
            &f.host,
            &instance,
            f.package.digest(),
            f.manager.revision(),
            &delete_caps(),
            90,
            1,
        )
        .unwrap();
    let path = f._dir.path().join("delete-target.bin");
    fs::write(&path, b"delete only this original").unwrap();
    if readonly {
        let mut permissions = fs::metadata(&path).unwrap().permissions();
        permissions.set_readonly(true);
        fs::set_permissions(&path, permissions).unwrap();
    }
    let mut broker = TargetBroker::new([0x71; 32]).unwrap();
    let selected = broker
        .select_existing(
            &f.manager,
            &f.host,
            &instance,
            &binding,
            &path,
            SelectionScope {
                subject: DELETE_SUBJECT.into(),
                approval_sha256: DELETE_APPROVAL,
                disposition: Disposition::Delete,
            },
            || 2,
        )
        .unwrap();
    let request = delete_request(&f, selected);
    broker
        .prepare_request(&f.manager, &mut f.host, &instance, &request, || 3)
        .unwrap();
    assert_eq!(
        f.host
            .store_local()
            .lookup_io_intent(DELETE_SUBJECT, DELETE_OP)
            .unwrap()
            .unwrap()
            .phase(),
        Phase::Prepared
    );
    (f, instance, binding, broker, request, path)
}

pub(super) fn setup() -> (
    Fixture,
    ManagedInstance,
    IoBinding,
    TargetBroker,
    RequestRecord,
    PathBuf,
) {
    setup_with_readonly(false)
}

pub(super) fn phase(f: &Fixture) -> Phase {
    f.host
        .store_local()
        .lookup_io_intent(DELETE_SUBJECT, DELETE_OP)
        .unwrap()
        .unwrap()
        .phase()
}

pub(super) fn observed(f: &Fixture, request: &RequestRecord) -> DeleteOutcome {
    assert_eq!(phase(f), Phase::Observed);
    let material = f
        .host
        .store_local()
        .io_material(DELETE_SUBJECT, DELETE_OP, Kind::Response)
        .unwrap()
        .unwrap();
    let value = DeleteOutcome::decode(material.payload()).unwrap();
    let expected = request.request();
    assert_eq!(value.operation_id(), DELETE_OP);
    assert_eq!(value.subject(), DELETE_SUBJECT);
    assert_eq!(
        value.request_sha256(),
        request.command().unwrap().request_sha256
    );
    assert_eq!(value.target_reference(), expected.target.reference);
    assert_eq!(
        value.expected_identity(),
        expected.expected_identity.unwrap()
    );
    f.host.store_local().integrity_check().unwrap();
    value
}

#[test]
fn real_delete_commits_unknown_before_effect_then_observed_response() {
    let _serial = serial_effects();
    let (mut f, instance, binding, mut broker, request, path) = setup();
    let prior = f.host.store_local().pending(0, 10).unwrap();
    assert_eq!(prior.len(), 1);
    let outcome = broker
        .delete(&f.manager, &mut f.host, &instance, &request, || 4)
        .unwrap();
    assert_eq!(outcome.result(), DeleteResult::Deleted);
    assert!(!path.exists());
    assert_eq!(binding.usage().resources, 0);
    let saved = observed(&f, &request);
    assert_eq!(saved.container(), outcome.container());
    let pending = f.host.store_local().pending(0, 10).unwrap();
    assert_eq!(pending.len(), 3);
    assert_eq!(pending[0], prior[0]);
    assert_eq!(broker.len(), 0);
    instance.close(&mut f.host).unwrap();
}

#[test]
fn deleting_one_selected_hardlink_keeps_its_alias_and_original_bytes() {
    let _serial = serial_effects();
    let mut f = Fixture::with_capabilities(true, 2, 100_000, delete_caps());
    let instance = f.connect();
    let binding = f
        .manager
        .bind_io(
            &f.host,
            &instance,
            f.package.digest(),
            f.manager.revision(),
            &delete_caps(),
            90,
            1,
        )
        .unwrap();
    let victim = f._dir.path().join("victim.bin");
    let alias = f._dir.path().join("alias.bin");
    let bytes = b"one hardlink must survive";
    fs::write(&victim, bytes).unwrap();
    fs::hard_link(&victim, &alias).unwrap();
    let mut broker = TargetBroker::new([0x73; 32]).unwrap();
    let selected = broker
        .select_existing(
            &f.manager,
            &f.host,
            &instance,
            &binding,
            &victim,
            SelectionScope {
                subject: DELETE_SUBJECT.into(),
                approval_sha256: DELETE_APPROVAL,
                disposition: Disposition::Delete,
            },
            || 2,
        )
        .unwrap();
    let request = delete_request(&f, selected);
    broker
        .prepare_request(&f.manager, &mut f.host, &instance, &request, || 3)
        .unwrap();
    let outcome = broker
        .delete(&f.manager, &mut f.host, &instance, &request, || 4)
        .unwrap();
    assert_eq!(outcome.result(), DeleteResult::Deleted);
    assert!(!victim.exists());
    assert_eq!(fs::read(&alias).unwrap(), bytes);
    assert_eq!(observed(&f, &request).container(), outcome.container());
    instance.close(&mut f.host).unwrap();
}

#[test]
fn readonly_native_rejection_is_observed_and_never_replayed() {
    let _serial = serial_effects();
    // Set before selection: the broker's exclusive handle intentionally denies
    // concurrent attribute changes through the pathname.
    let (mut f, instance, _binding, mut broker, request, path) = setup_with_readonly(true);
    let outcome = broker
        .delete(&f.manager, &mut f.host, &instance, &request, || 4)
        .unwrap();
    assert!(matches!(
        outcome.result(),
        DeleteResult::OsRejected { code: 1.. }
    ));
    assert!(path.exists());
    assert_eq!(observed(&f, &request).container(), outcome.container());
    assert!(matches!(
        broker.delete(&f.manager, &mut f.host, &instance, &request, || 5),
        Err(DeleteError::AlreadyDispatched)
    ));
    assert!(path.exists());
    let mut permissions = fs::metadata(&path).unwrap().permissions();
    // This entire test module is cfg(windows); no Unix permission widening is possible.
    #[allow(clippy::permissions_set_readonly_false)]
    permissions.set_readonly(false);
    fs::set_permissions(&path, permissions).unwrap();
    instance.close(&mut f.host).unwrap();
}

#[test]
fn mismatched_action_plan_and_foreign_owner_cannot_cross_claim() {
    let _serial = serial_effects();
    let (mut f, instance, _binding, mut broker, request, path) = setup();
    let foreign = f.connect();
    assert!(matches!(
        broker.delete(&f.manager, &mut f.host, &foreign, &request, || 4),
        Err(DeleteError::Admission(AdmissionError::Denied))
    ));
    let mut changed = request.request().clone();
    changed.subject = "another-subject".into();
    let changed = RequestRecord::new(changed).unwrap();
    assert!(matches!(
        broker.delete(&f.manager, &mut f.host, &instance, &changed, || 4),
        Err(DeleteError::Mismatch)
    ));
    let mut changed = request.request().clone();
    changed.target.reference = [8; 32];
    let changed = RequestRecord::new(changed).unwrap();
    assert!(matches!(
        broker.delete(&f.manager, &mut f.host, &instance, &changed, || 4),
        Err(DeleteError::Missing)
    ));
    let mut changed = request.request().clone();
    changed.disposition = Disposition::Replace;
    changed.content_length = 1;
    changed.content_sha256 = Some(morrow_core::runtime::schema_digest(b"x"));
    let changed = RequestRecord::new(changed).unwrap();
    assert!(matches!(
        broker.delete(&f.manager, &mut f.host, &instance, &changed, || 4),
        Err(DeleteError::Mismatch)
    ));
    assert_eq!(phase(&f), Phase::Prepared);
    assert_eq!(fs::read(&path).unwrap_err().raw_os_error(), Some(32));
    foreign.close(&mut f.host).unwrap();
    instance.close(&mut f.host).unwrap();
}

#[test]
fn preclaim_revocation_rolls_back_without_touching_file() {
    let _serial = serial_effects();
    for revoke in ["stop", "expiry"] {
        let (mut f, instance, _binding, mut broker, request, path) = setup();
        let mut ticks = 0;
        let result = broker.delete(&f.manager, &mut f.host, &instance, &request, || {
            ticks += 1;
            // Three validate_request samples, then one admit, then the first
            // claim transaction liveness guard.
            if ticks == 5 {
                if revoke == "stop" {
                    instance.stop();
                } else {
                    return 90;
                }
            }
            4
        });
        assert!(matches!(result, Err(DeleteError::Admission(_))), "{revoke}");
        assert_eq!(phase(&f), Phase::Prepared);
        assert_eq!(broker.len(), 1);
        broker.reap(if revoke == "expiry" { 90 } else { 4 });
        assert_eq!(fs::read(&path).unwrap(), b"delete only this original");
        instance.close(&mut f.host).unwrap();
    }
}

fn successful_delete_samples() -> usize {
    let (mut f, instance, _binding, mut broker, request, path) = setup();
    let mut ticks = 0;
    let outcome = broker
        .delete(&f.manager, &mut f.host, &instance, &request, || {
            ticks += 1;
            4
        })
        .unwrap();
    assert_eq!(outcome.result(), DeleteResult::Deleted);
    assert!(!path.exists());
    assert!(ticks >= 9);
    ticks
}

#[test]
fn after_claim_loss_is_unknown_and_retry_never_reaches_native_delete() {
    let _serial = serial_effects();
    let samples = successful_delete_samples();
    for revoke in ["stop", "expiry"] {
        let (mut f, instance, _binding, mut broker, request, path) = setup();
        let mut ticks = 0;
        let result = broker.delete(&f.manager, &mut f.host, &instance, &request, || {
            ticks += 1;
            if ticks == samples - 2 {
                if revoke == "stop" {
                    instance.stop();
                } else {
                    return 90;
                }
            }
            4
        });
        assert!(
            matches!(result, Err(DeleteError::OutcomeUnknown)),
            "{revoke}"
        );
        assert_eq!(phase(&f), Phase::OutcomeUnknown);
        assert!(path.exists());
        assert_eq!(broker.len(), 0);
        assert!(matches!(
            broker.delete(&f.manager, &mut f.host, &instance, &request, || 90),
            Err(DeleteError::Admission(_))
        ));
        assert!(path.exists());
        f.host.store_local().integrity_check().unwrap();
        instance.close(&mut f.host).unwrap();
    }
}

#[test]
fn effect_or_observation_followed_by_revocation_preserves_observed_history() {
    let _serial = serial_effects();
    let samples = successful_delete_samples();
    for revocation_at in [samples - 1, samples] {
        let (mut f, instance, _binding, mut broker, request, path) = setup();
        let mut ticks = 0;
        let result = broker.delete(&f.manager, &mut f.host, &instance, &request, || {
            ticks += 1;
            if ticks == revocation_at {
                instance.stop();
            }
            4
        });
        assert!(matches!(
            result,
            Err(DeleteError::CommittedButDeliveryDenied(
                AdmissionError::Denied
            ))
        ));
        assert!(!path.exists());
        assert_eq!(observed(&f, &request).result(), DeleteResult::Deleted);
        assert_eq!(broker.len(), 0);
        instance.close(&mut f.host).unwrap();
    }
}

#[test]
fn same_operation_cannot_delete_a_later_file_at_the_same_path() {
    let _serial = serial_effects();
    let (mut f, instance, binding, mut broker, request, path) = setup();
    assert_eq!(
        broker
            .delete(&f.manager, &mut f.host, &instance, &request, || 4)
            .unwrap()
            .result(),
        DeleteResult::Deleted
    );
    fs::write(&path, b"new unrelated entry").unwrap();
    assert!(matches!(
        broker.delete(&f.manager, &mut f.host, &instance, &request, || 5),
        Err(DeleteError::AlreadyDispatched)
    ));
    let mut other = TargetBroker::new([0x72; 32]).unwrap();
    let selected = other
        .select_existing(
            &f.manager,
            &f.host,
            &instance,
            &binding,
            &path,
            SelectionScope {
                subject: DELETE_SUBJECT.into(),
                approval_sha256: DELETE_APPROVAL,
                disposition: Disposition::Delete,
            },
            || 5,
        )
        .unwrap();
    let another = delete_request(&f, selected);
    assert!(
        other
            .delete(&f.manager, &mut f.host, &instance, &another, || 6)
            .is_err()
    );
    drop(other);
    assert_eq!(fs::read(&path).unwrap(), b"new unrelated entry");
    assert_eq!(observed(&f, &request).result(), DeleteResult::Deleted);
    instance.close(&mut f.host).unwrap();
}

#[test]
fn lost_broker_does_not_restore_old_selection_but_observation_remains_readable() {
    let _serial = serial_effects();
    let (mut f, instance, _binding, mut broker, request, path) = setup();
    broker
        .delete(&f.manager, &mut f.host, &instance, &request, || 4)
        .unwrap();
    drop(broker);
    let mut fresh = TargetBroker::new([0x71; 32]).unwrap();
    assert!(matches!(
        fresh.delete(&f.manager, &mut f.host, &instance, &request, || 5),
        Err(DeleteError::Missing)
    ));
    assert_eq!(observed(&f, &request).result(), DeleteResult::Deleted);
    assert!(!path.exists());
    instance.close(&mut f.host).unwrap();
}

#[test]
fn reentrant_delete_on_another_host_is_busy_without_claim_or_effect() {
    let _serial = serial_effects();
    let (mut first, first_instance, _first_binding, mut first_broker, first_request, first_path) =
        setup();
    let (
        mut second,
        second_instance,
        _second_binding,
        mut second_broker,
        second_request,
        second_path,
    ) = setup();
    let mut ticks = 0;
    let mut attempted = false;
    let first_outcome = first_broker
        .delete(
            &first.manager,
            &mut first.host,
            &first_instance,
            &first_request,
            || {
                ticks += 1;
                if ticks == 4 {
                    attempted = true;
                    assert!(matches!(
                        second_broker.delete(
                            &second.manager,
                            &mut second.host,
                            &second_instance,
                            &second_request,
                            || 4
                        ),
                        Err(DeleteError::Busy)
                    ));
                    assert_eq!(phase(&second), Phase::Prepared);
                    assert!(second_path.exists());
                    assert_eq!(second_broker.len(), 1);
                }
                4
            },
        )
        .unwrap();
    assert!(attempted);
    assert_eq!(first_outcome.result(), DeleteResult::Deleted);
    assert!(!first_path.exists());
    assert_eq!(
        observed(&first, &first_request).result(),
        DeleteResult::Deleted
    );
    assert_eq!(phase(&second), Phase::Prepared);
    let second_outcome = second_broker
        .delete(
            &second.manager,
            &mut second.host,
            &second_instance,
            &second_request,
            || 5,
        )
        .unwrap();
    assert_eq!(second_outcome.result(), DeleteResult::Deleted);
    assert!(!second_path.exists());
    assert_eq!(
        observed(&second, &second_request).result(),
        DeleteResult::Deleted
    );
    first_instance.close(&mut first.host).unwrap();
    second_instance.close(&mut second.host).unwrap();
}
