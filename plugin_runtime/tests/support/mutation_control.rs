//! Real Windows cancellation boundaries for selected Create and Delete.
use super::*;
use morrow_core::{
    file_effect::{CreateResult, DeleteResult},
    file_mutation::RequestRecord,
    io_intent::Phase,
};
use morrow_plugin_runtime::file_target::{Error as TargetError, TargetControl};
use std::{
    fs,
    path::{Path, PathBuf},
};

struct ScriptedControl {
    cancel_at: usize,
    samples: usize,
    now: u64,
}
impl ScriptedControl {
    fn live() -> Self {
        Self {
            cancel_at: usize::MAX,
            samples: 0,
            now: 5,
        }
    }
    fn cancel_at(sample: usize) -> Self {
        assert!(sample > 0);
        Self {
            cancel_at: sample,
            samples: 0,
            now: 5,
        }
    }
}
impl TargetControl for ScriptedControl {
    fn with<T>(&mut self, action: impl FnOnce(u64, bool) -> T) -> T {
        self.samples += 1;
        action(self.now, self.samples >= self.cancel_at)
    }
}

fn temporary(leaf: &Path, request: &RequestRecord) -> PathBuf {
    let digest = request.command().unwrap().request_sha256;
    leaf.parent().unwrap().join(format!(
        ".morrow-create-{}.tmp",
        digest
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>()
    ))
}

fn create_samples(bytes: &[u8]) -> usize {
    let (mut f, instance, _, mut broker, request, leaf) =
        super::file_create::setup(bytes, "sample.bin", 2);
    super::file_create::stage(&mut f, &instance, &broker, &request, bytes);
    let mut control = ScriptedControl::live();
    let result = broker
        .create_controlled(&f.manager, &mut f.host, &instance, &request, &mut control)
        .unwrap();
    assert_eq!(result.result(), CreateResult::Created);
    assert_eq!(fs::read(leaf).unwrap(), bytes);
    instance.close(&mut f.host).unwrap();
    control.samples
}

fn delete_samples() -> usize {
    let (mut f, instance, _, mut broker, request, path) = super::file_delete::setup();
    let mut control = ScriptedControl::live();
    let result = broker
        .delete_controlled(&f.manager, &mut f.host, &instance, &request, &mut control)
        .unwrap();
    assert_eq!(result.result(), DeleteResult::Deleted);
    assert!(!path.exists());
    instance.close(&mut f.host).unwrap();
    control.samples
}

#[test]
fn cancelled_on_entry_keeps_prepared_and_releases_only_explicitly_selected_resources() {
    let _serial = super::file_delete::serial_effects();

    let (mut f, instance, binding, mut broker, request, leaf) =
        super::file_create::setup(b"entry", "entry.bin", 2);
    super::file_create::stage(&mut f, &instance, &broker, &request, b"entry");
    let pending = f.host.store_local().pending(0, 10).unwrap();
    let mut control = ScriptedControl::cancel_at(1);
    assert!(matches!(
        broker.create_controlled(&f.manager, &mut f.host, &instance, &request, &mut control),
        Err(TargetError::CancelledBeforeDispatch)
    ));
    assert_eq!(control.samples, 1);
    assert_eq!(super::file_create::phase(&f), Phase::Prepared);
    assert_eq!(f.host.store_local().pending(0, 10).unwrap(), pending);
    assert!(!leaf.exists());
    assert!(!temporary(&leaf, &request).exists());
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
    instance.close(&mut f.host).unwrap();

    let (mut f, instance, binding, mut broker, request, path) = super::file_delete::setup();
    let pending = f.host.store_local().pending(0, 10).unwrap();
    let mut control = ScriptedControl::cancel_at(1);
    assert!(matches!(
        broker.delete_controlled(&f.manager, &mut f.host, &instance, &request, &mut control),
        Err(TargetError::CancelledBeforeDispatch)
    ));
    assert_eq!(control.samples, 1);
    assert_eq!(super::file_delete::phase(&f), Phase::Prepared);
    assert_eq!(f.host.store_local().pending(0, 10).unwrap(), pending);
    broker
        .release(
            &f.manager,
            &f.host,
            &instance,
            request.request().target.reference,
            5,
        )
        .unwrap();
    assert_eq!(fs::read(&path).unwrap(), b"delete only this original");
    assert_eq!(binding.usage().resources, 0);
    instance.close(&mut f.host).unwrap();
}

#[test]
fn cancellation_at_create_claim_guard_does_not_commit_unknown_or_open_a_temp() {
    let _serial = super::file_delete::serial_effects();
    let count = create_samples(b"claim guard");
    // One-chunk Create ends with four native checks, post-effect sample,
    // and delivery. The preceding sample is the last claim authorization.
    assert!(count >= 7);
    let (mut f, instance, _, mut broker, request, leaf) =
        super::file_create::setup(b"claim guard", "guard.bin", 2);
    super::file_create::stage(&mut f, &instance, &broker, &request, b"claim guard");
    let pending = f.host.store_local().pending(0, 10).unwrap();
    let mut control = ScriptedControl::cancel_at(count - 6);
    assert!(
        matches!(
            broker.create_controlled(&f.manager, &mut f.host, &instance, &request, &mut control),
            Err(TargetError::CancelledBeforeDispatch)
        ),
        "samples={}",
        control.samples
    );
    assert_eq!(super::file_create::phase(&f), Phase::Prepared);
    assert_eq!(f.host.store_local().pending(0, 10).unwrap(), pending);
    assert!(!leaf.exists());
    assert!(!temporary(&leaf, &request).exists());
    assert_eq!(broker.len(), 1);
    instance.close(&mut f.host).unwrap();
}

#[test]
fn cancellation_at_delete_claim_guard_preserves_original_file_and_prepared_history() {
    let _serial = super::file_delete::serial_effects();
    let count = delete_samples();
    // Delete ends with two pre-effect checks, post-effect sample and delivery.
    assert!(count >= 5);
    let (mut f, instance, _, mut broker, request, path) = super::file_delete::setup();
    let pending = f.host.store_local().pending(0, 10).unwrap();
    let mut control = ScriptedControl::cancel_at(count - 4);
    assert!(
        matches!(
            broker.delete_controlled(&f.manager, &mut f.host, &instance, &request, &mut control),
            Err(TargetError::CancelledBeforeDispatch)
        ),
        "samples={}",
        control.samples
    );
    assert_eq!(super::file_delete::phase(&f), Phase::Prepared);
    assert_eq!(f.host.store_local().pending(0, 10).unwrap(), pending);
    broker
        .release(
            &f.manager,
            &f.host,
            &instance,
            request.request().target.reference,
            5,
        )
        .unwrap();
    assert_eq!(fs::read(&path).unwrap(), b"delete only this original");
    instance.close(&mut f.host).unwrap();
}

#[test]
fn create_cancel_after_claim_during_second_chunk_or_before_publish_is_unknown_and_cleans_temp() {
    let _serial = super::file_delete::serial_effects();
    let bytes = vec![0x5d; 64 * 1024 + 37];
    let count = create_samples(&bytes);
    // Two chunks: pre-open N-6, chunk one N-5, chunk two N-4,
    // pre-sync N-3, pre-publish N-2, post-effect N-1, delivery N.
    assert!(count >= 8);
    for (label, sample) in [
        ("after-claim", count - 6),
        ("second-chunk", count - 4),
        ("before-publish", count - 2),
    ] {
        let (mut f, instance, _, mut broker, request, leaf) =
            super::file_create::setup(&bytes, "unknown.bin", 2);
        super::file_create::stage(&mut f, &instance, &broker, &request, &bytes);
        let mut control = ScriptedControl::cancel_at(sample);
        assert!(
            matches!(
                broker.create_controlled(
                    &f.manager,
                    &mut f.host,
                    &instance,
                    &request,
                    &mut control
                ),
                Err(TargetError::OutcomeUnknown)
            ),
            "{label}: samples={}",
            control.samples
        );
        assert_eq!(
            super::file_create::phase(&f),
            Phase::OutcomeUnknown,
            "{label}"
        );
        assert!(!leaf.exists(), "{label}");
        assert!(!temporary(&leaf, &request).exists(), "{label}");
        assert_eq!(broker.len(), 0, "{label}");
        let mut retry = ScriptedControl::live();
        assert!(
            matches!(
                broker.create_controlled(&f.manager, &mut f.host, &instance, &request, &mut retry),
                Err(TargetError::AlreadyDispatched)
            ),
            "{label}"
        );
        f.host.store_local().integrity_check().unwrap();
        instance.close(&mut f.host).unwrap();
    }
}

#[test]
fn delete_cancel_after_claim_never_reaches_native_delete_or_replays() {
    let _serial = super::file_delete::serial_effects();
    let count = delete_samples();
    for (label, sample) in [("after-claim", count - 3), ("before-effect", count - 2)] {
        let (mut f, instance, _, mut broker, request, path) = super::file_delete::setup();
        let mut control = ScriptedControl::cancel_at(sample);
        assert!(
            matches!(
                broker.delete_controlled(
                    &f.manager,
                    &mut f.host,
                    &instance,
                    &request,
                    &mut control
                ),
                Err(TargetError::OutcomeUnknown)
            ),
            "{label}: samples={}",
            control.samples
        );
        assert_eq!(
            super::file_delete::phase(&f),
            Phase::OutcomeUnknown,
            "{label}"
        );
        assert_eq!(
            fs::read(&path).unwrap(),
            b"delete only this original",
            "{label}"
        );
        assert_eq!(broker.len(), 0);
        let mut retry = ScriptedControl::live();
        assert!(
            matches!(
                broker.delete_controlled(&f.manager, &mut f.host, &instance, &request, &mut retry),
                Err(TargetError::AlreadyDispatched)
            ),
            "{label}"
        );
        f.host.store_local().integrity_check().unwrap();
        instance.close(&mut f.host).unwrap();
    }
}

#[test]
fn cancellation_after_effect_or_observation_keeps_created_file_and_observed_response() {
    let _serial = super::file_delete::serial_effects();
    let bytes = b"observed after cancellation";
    let count = create_samples(bytes);
    for (label, sample) in [("after-effect", count - 1), ("after-observe", count)] {
        let (mut f, instance, _, mut broker, request, leaf) =
            super::file_create::setup(bytes, "observed.bin", 2);
        super::file_create::stage(&mut f, &instance, &broker, &request, bytes);
        let mut control = ScriptedControl::cancel_at(sample);
        assert!(
            matches!(
                broker.create_controlled(
                    &f.manager,
                    &mut f.host,
                    &instance,
                    &request,
                    &mut control
                ),
                Err(TargetError::CommittedButDeliveryCancelled)
            ),
            "{label}: samples={}",
            control.samples
        );
        assert_eq!(fs::read(&leaf).unwrap(), bytes, "{label}");
        assert_eq!(super::file_create::phase(&f), Phase::Observed);
        assert_eq!(
            super::file_create::observed(&f, &request).result(),
            CreateResult::Created
        );
        assert!(!temporary(&leaf, &request).exists());
        instance.close(&mut f.host).unwrap();
    }
}

#[test]
fn cancellation_after_effect_or_observation_keeps_deleted_file_observed() {
    let _serial = super::file_delete::serial_effects();
    let count = delete_samples();
    for (label, sample) in [("after-effect", count - 1), ("after-observe", count)] {
        let (mut f, instance, _, mut broker, request, path) = super::file_delete::setup();
        let mut control = ScriptedControl::cancel_at(sample);
        assert!(
            matches!(
                broker.delete_controlled(
                    &f.manager,
                    &mut f.host,
                    &instance,
                    &request,
                    &mut control
                ),
                Err(TargetError::CommittedButDeliveryCancelled)
            ),
            "{label}: samples={}",
            control.samples
        );
        assert!(!path.exists(), "{label}");
        assert_eq!(super::file_delete::phase(&f), Phase::Observed);
        assert_eq!(
            super::file_delete::observed(&f, &request).result(),
            DeleteResult::Deleted
        );
        instance.close(&mut f.host).unwrap();
    }
}

#[test]
fn cancelled_stage_does_not_write_content_and_can_be_retried_by_live_owner() {
    let _serial = super::file_delete::serial_effects();
    let bytes = b"stage only under live control";
    let (mut f, instance, _, broker, request, leaf) =
        super::file_create::setup(bytes, "stage.bin", 2);
    let pending = f.host.store_local().pending(0, 10).unwrap();
    let mut control = ScriptedControl::cancel_at(1);
    assert!(matches!(
        broker.stage_content_controlled(
            &f.manager,
            &mut f.host,
            &instance,
            &request,
            bytes,
            &mut control
        ),
        Err(TargetError::CancelledBeforeDispatch)
    ));
    assert_eq!(super::file_create::phase(&f), Phase::Prepared);
    assert_eq!(f.host.store_local().pending(0, 10).unwrap(), pending);
    assert_eq!(
        f.host
            .store_local()
            .file_mutation_content_usage()
            .unwrap()
            .0,
        0
    );
    assert!(!leaf.exists());
    let mut live = ScriptedControl::live();
    broker
        .stage_content_controlled(
            &f.manager,
            &mut f.host,
            &instance,
            &request,
            bytes,
            &mut live,
        )
        .unwrap();
    assert_eq!(
        f.host
            .store_local()
            .file_mutation_content_usage()
            .unwrap()
            .0,
        1
    );
    instance.close(&mut f.host).unwrap();
}

struct LockWitness<'a> {
    clock: std::sync::Arc<std::sync::Mutex<u64>>,
    binding: &'a IoBinding,
    samples: usize,
    resource_change_inside: bool,
    job_or_byte_change_inside: bool,
}
impl<'a> LockWitness<'a> {
    fn new(binding: &'a IoBinding) -> Self {
        Self {
            clock: std::sync::Arc::new(std::sync::Mutex::new(5)),
            binding,
            samples: 0,
            resource_change_inside: false,
            job_or_byte_change_inside: false,
        }
    }
}
impl TargetControl for LockWitness<'_> {
    fn with<T>(&mut self, action: impl FnOnce(u64, bool) -> T) -> T {
        let guard = self.clock.lock().unwrap();
        let probe = std::sync::Arc::clone(&self.clock);
        let blocked = std::thread::spawn(move || {
            matches!(probe.try_lock(), Err(std::sync::TryLockError::WouldBlock))
        })
        .join()
        .unwrap();
        assert!(
            blocked,
            "another thread acquired the clock during a control action"
        );
        let before = self.binding.usage();
        let value = action(*guard, false);
        let after = self.binding.usage();
        self.samples += 1;
        self.resource_change_inside |= after.resources > before.resources;
        self.job_or_byte_change_inside |= after.jobs > before.jobs || after.bytes > before.bytes;
        drop(guard);
        value
    }
}

#[test]
fn actual_create_and_delete_admission_happens_inside_the_serialized_clock_action() {
    let _serial = super::file_delete::serial_effects();
    let bytes = b"lock witness";

    let (mut f, instance, binding, mut broker, request, leaf) =
        super::file_create::setup(bytes, "witness.bin", 2);
    super::file_create::stage(&mut f, &instance, &broker, &request, bytes);
    let mut control = LockWitness::new(&binding);
    let outcome = broker
        .create_controlled(&f.manager, &mut f.host, &instance, &request, &mut control)
        .unwrap();
    assert_eq!(outcome.result(), CreateResult::Created);
    assert_eq!(fs::read(&leaf).unwrap(), bytes);
    assert!(control.samples >= 10);
    assert!(
        control.resource_change_inside,
        "temporary resource admission escaped the clock action"
    );
    assert!(
        control.job_or_byte_change_inside,
        "create job/byte admission escaped the clock action"
    );
    instance.close(&mut f.host).unwrap();

    let (mut f, instance, binding, mut broker, request, path) = super::file_delete::setup();
    let mut control = LockWitness::new(&binding);
    let outcome = broker
        .delete_controlled(&f.manager, &mut f.host, &instance, &request, &mut control)
        .unwrap();
    assert_eq!(outcome.result(), DeleteResult::Deleted);
    assert!(!path.exists());
    assert!(control.samples >= 9);
    assert!(
        control.job_or_byte_change_inside,
        "delete job/byte admission escaped the clock action"
    );
    instance.close(&mut f.host).unwrap();
}

#[test]
fn cancelled_reentry_preserves_prior_observed_effects_and_never_replays_them() {
    let _serial = super::file_delete::serial_effects();
    let bytes = b"first create effect";
    let (mut f, instance, _, mut broker, request, leaf) =
        super::file_create::setup(bytes, "spent-create.bin", 2);
    super::file_create::stage(&mut f, &instance, &broker, &request, bytes);
    broker
        .create_controlled(
            &f.manager,
            &mut f.host,
            &instance,
            &request,
            &mut ScriptedControl::live(),
        )
        .unwrap();
    assert_eq!(
        super::file_create::observed(&f, &request).result(),
        CreateResult::Created
    );
    // A missing leaf makes a mistaken second Create visible.
    fs::remove_file(&leaf).unwrap();
    let pending = f.host.store_local().pending(0, 10).unwrap();
    let mut cancelled = ScriptedControl::cancel_at(1);
    assert!(matches!(
        broker.create_controlled(&f.manager, &mut f.host, &instance, &request, &mut cancelled),
        Err(TargetError::AlreadyDispatched)
    ));
    assert_eq!(cancelled.samples, 1);
    assert!(!leaf.exists());
    assert!(!temporary(&leaf, &request).exists());
    assert_eq!(super::file_create::phase(&f), Phase::Observed);
    assert_eq!(f.host.store_local().pending(0, 10).unwrap(), pending);
    instance.close(&mut f.host).unwrap();

    let (mut f, instance, _, mut broker, request, path) = super::file_delete::setup();
    broker
        .delete_controlled(
            &f.manager,
            &mut f.host,
            &instance,
            &request,
            &mut ScriptedControl::live(),
        )
        .unwrap();
    assert_eq!(
        super::file_delete::observed(&f, &request).result(),
        DeleteResult::Deleted
    );
    // A later entry with the same name must survive the spent operation.
    fs::write(&path, b"later unrelated file").unwrap();
    let pending = f.host.store_local().pending(0, 10).unwrap();
    let mut cancelled = ScriptedControl::cancel_at(1);
    assert!(matches!(
        broker.delete_controlled(&f.manager, &mut f.host, &instance, &request, &mut cancelled),
        Err(TargetError::AlreadyDispatched)
    ));
    assert_eq!(cancelled.samples, 1);
    assert_eq!(fs::read(&path).unwrap(), b"later unrelated file");
    assert_eq!(super::file_delete::phase(&f), Phase::Observed);
    assert_eq!(f.host.store_local().pending(0, 10).unwrap(), pending);
    instance.close(&mut f.host).unwrap();
}

#[test]
fn cancelled_reentry_after_unknown_claim_reports_already_dispatched_without_effect() {
    let _serial = super::file_delete::serial_effects();
    let bytes = b"never published";
    let create_count = create_samples(bytes);
    let (mut f, instance, _, mut broker, request, leaf) =
        super::file_create::setup(bytes, "unknown-reentry.bin", 2);
    super::file_create::stage(&mut f, &instance, &broker, &request, bytes);
    let mut first = ScriptedControl::cancel_at(create_count - 5);
    assert!(matches!(
        broker.create_controlled(&f.manager, &mut f.host, &instance, &request, &mut first),
        Err(TargetError::OutcomeUnknown)
    ));
    assert_eq!(super::file_create::phase(&f), Phase::OutcomeUnknown);
    let pending = f.host.store_local().pending(0, 10).unwrap();
    let mut again = ScriptedControl::cancel_at(1);
    assert!(matches!(
        broker.create_controlled(&f.manager, &mut f.host, &instance, &request, &mut again),
        Err(TargetError::AlreadyDispatched)
    ));
    assert_eq!(again.samples, 1);
    assert_eq!(super::file_create::phase(&f), Phase::OutcomeUnknown);
    assert_eq!(f.host.store_local().pending(0, 10).unwrap(), pending);
    assert!(!leaf.exists());
    assert!(!temporary(&leaf, &request).exists());
    instance.close(&mut f.host).unwrap();

    let delete_count = delete_samples();
    let (mut f, instance, _, mut broker, request, path) = super::file_delete::setup();
    let mut first = ScriptedControl::cancel_at(delete_count - 3);
    assert!(matches!(
        broker.delete_controlled(&f.manager, &mut f.host, &instance, &request, &mut first),
        Err(TargetError::OutcomeUnknown)
    ));
    assert_eq!(super::file_delete::phase(&f), Phase::OutcomeUnknown);
    let pending = f.host.store_local().pending(0, 10).unwrap();
    let mut again = ScriptedControl::cancel_at(1);
    assert!(matches!(
        broker.delete_controlled(&f.manager, &mut f.host, &instance, &request, &mut again),
        Err(TargetError::AlreadyDispatched)
    ));
    assert_eq!(again.samples, 1);
    assert_eq!(super::file_delete::phase(&f), Phase::OutcomeUnknown);
    assert_eq!(f.host.store_local().pending(0, 10).unwrap(), pending);
    assert_eq!(fs::read(&path).unwrap(), b"delete only this original");
    instance.close(&mut f.host).unwrap();
}
