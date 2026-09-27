//! Real Windows handle selection. This verifies admission and lifetime, never dispatch.
use super::*;
use morrow_core::{
    file_mutation::{Disposition, MutationRequest, RequestRecord, Target},
    file_path::RelativeFilePath,
};
use morrow_plugin_runtime::{
    file_io::FileBroker,
    file_target::{Error as TargetError, SelectedTarget, SelectionScope, TargetBroker},
    io_binding::Error as AdmissionError,
};
use sha2::{Digest, Sha256};
use std::{fs, path::Path};

const SUBJECT: &str = "plugin.mutation-target-test";
const APPROVAL: [u8; 32] = [0xa4; 32];

fn mutation_caps() -> BTreeSet<IoCapability> {
    BTreeSet::from([IoCapability::FileReplace, IoCapability::FileDelete])
}

fn binding(f: &Fixture, instance: &ManagedInstance, caps: &BTreeSet<IoCapability>) -> IoBinding {
    f.manager
        .bind_io(
            &f.host,
            instance,
            f.package.digest(),
            f.manager.revision(),
            caps,
            90,
            1,
        )
        .unwrap()
}

fn scope(disposition: Disposition) -> SelectionScope {
    SelectionScope {
        subject: SUBJECT.into(),
        approval_sha256: APPROVAL,
        disposition,
    }
}

fn plan(f: &Fixture, selected: SelectedTarget, disposition: Disposition) -> RequestRecord {
    let bytes = b"future replacement bytes";
    RequestRecord::new(MutationRequest {
        operation_id: "replace-selected-target".into(),
        subject: SUBJECT.into(),
        package_sha256: f.package.digest(),
        approval_sha256: APPROVAL,
        target: Target {
            reference: selected.reference,
            relative_path: None,
        },
        disposition,
        expected_identity: Some(selected.expected_identity),
        content_length: if disposition == Disposition::Delete {
            0
        } else {
            bytes.len() as u64
        },
        content_sha256: if disposition == Disposition::Delete {
            None
        } else {
            Some(Sha256::digest(bytes).into())
        },
    })
    .unwrap()
}

fn select(
    broker: &mut TargetBroker,
    f: &Fixture,
    instance: &ManagedInstance,
    binding: &IoBinding,
    path: &Path,
    disposition: Disposition,
) -> SelectedTarget {
    broker
        .select_existing(
            &f.manager,
            &f.host,
            instance,
            binding,
            path,
            scope(disposition),
            || 2,
        )
        .unwrap()
}

#[test]
fn selected_handle_is_exclusive_and_release_or_drop_restores_file_access() {
    assert_eq!(
        TargetBroker::new([0; 32]).err(),
        Some(TargetError::InvalidSelection)
    );
    for disposition in [Disposition::Replace, Disposition::Delete] {
        let mut f = Fixture::with_capabilities(true, 2, 1000, mutation_caps());
        let instance = f.connect();
        let binding = binding(&f, &instance, &mutation_caps());
        let path = f._dir.path().join("selected.bin");
        let renamed = f._dir.path().join("renamed.bin");
        fs::write(&path, b"original bytes").unwrap();
        let mut broker = TargetBroker::new([0x31; 32]).unwrap();
        let selected = select(&mut broker, &f, &instance, &binding, &path, disposition);
        assert_eq!(broker.len(), 1);
        assert_eq!(binding.usage().resources, 1);
        assert_eq!(fs::read(&path).unwrap_err().raw_os_error(), Some(32));
        assert!(fs::write(&path, b"changed").is_err());
        assert!(fs::rename(&path, &renamed).is_err());
        assert!(fs::remove_file(&path).is_err());
        broker
            .validate_request(
                &f.manager,
                &f.host,
                &instance,
                &plan(&f, selected, disposition),
                || 3,
            )
            .unwrap();
        if disposition == Disposition::Replace {
            broker
                .release(&f.manager, &f.host, &instance, selected.reference, 4)
                .unwrap();
        } else {
            drop(broker);
        }
        assert_eq!(binding.usage().resources, 0);
        assert_eq!(fs::read(&path).unwrap(), b"original bytes");
        fs::write(&path, b"changed").unwrap();
        fs::rename(&path, &renamed).unwrap();
        fs::remove_file(&renamed).unwrap();
        instance.close(&mut f.host).unwrap();
    }
}

#[test]
fn exact_plan_binds_owner_scope_package_action_and_selected_identity() {
    let mut f = Fixture::with_capabilities(true, 2, 1000, mutation_caps());
    let owner = f.connect();
    let foreign = f.connect();
    let binding = binding(&f, &owner, &mutation_caps());
    let path = f._dir.path().join("selected.bin");
    fs::write(&path, b"original").unwrap();
    let mut broker = TargetBroker::new([0x32; 32]).unwrap();
    let selected = select(
        &mut broker,
        &f,
        &owner,
        &binding,
        &path,
        Disposition::Replace,
    );
    let valid = plan(&f, selected, Disposition::Replace);
    broker
        .validate_request(&f.manager, &f.host, &owner, &valid, || 3)
        .unwrap();
    assert_eq!(
        broker.validate_request(&f.manager, &f.host, &foreign, &valid, || 3),
        Err(TargetError::Admission(AdmissionError::Denied))
    );
    let other_host = HostRuntime::new(
        Store::open(&f._dir.path().join("other-host.db"), EventBudget::default()).unwrap(),
    )
    .unwrap();
    assert_eq!(
        broker.validate_request(&f.manager, &other_host, &owner, &valid, || 3),
        Err(TargetError::Admission(AdmissionError::Denied))
    );
    let variants = [
        "subject",
        "package",
        "approval",
        "action",
        "identity",
        "reference",
        "relative",
    ];
    for variant in variants {
        let mut changed = valid.request().clone();
        match variant {
            "subject" => changed.subject = "foreign-subject".into(),
            "package" => changed.package_sha256 = [9; 32],
            "approval" => changed.approval_sha256 = [8; 32],
            "action" => changed.disposition = Disposition::Delete,
            "identity" => changed.expected_identity = Some([7; 32]),
            "reference" => changed.target.reference = [6; 32],
            "relative" => {
                changed.target.relative_path = Some(RelativeFilePath::parse("other.bin").unwrap())
            }
            _ => unreachable!(),
        }
        if variant == "action" {
            changed.content_length = 0;
            changed.content_sha256 = None;
        }
        let changed = RequestRecord::new(changed).unwrap();
        let expected = if variant == "reference" {
            TargetError::Missing
        } else {
            TargetError::Mismatch
        };
        assert_eq!(
            broker.validate_request(&f.manager, &f.host, &owner, &changed, || 3),
            Err(expected),
            "{variant}"
        );
    }
    assert_eq!(
        broker.release(&f.manager, &f.host, &foreign, selected.reference, 4),
        Err(TargetError::Admission(AdmissionError::Denied))
    );
    assert_eq!(broker.len(), 1);
    broker
        .release(&f.manager, &f.host, &owner, selected.reference, 4)
        .unwrap();
    assert_eq!(
        broker.validate_request(&f.manager, &f.host, &owner, &valid, || 5),
        Err(TargetError::Missing)
    );
    foreign.close(&mut f.host).unwrap();
    owner.close(&mut f.host).unwrap();
}

#[test]
fn mutation_selection_and_read_capture_share_one_resource_budget() {
    let caps = BTreeSet::from([IoCapability::FileReplace, IoCapability::FileRead]);
    let mut f = Fixture::with_capabilities(true, 1, 1000, caps.clone());
    let instance = f.connect();
    let binding = binding(&f, &instance, &caps);
    let path = f._dir.path().join("selected.bin");
    fs::write(&path, b"original").unwrap();
    let mut targets = TargetBroker::new([0x33; 32]).unwrap();
    let selected = select(
        &mut targets,
        &f,
        &instance,
        &binding,
        &path,
        Disposition::Replace,
    );
    let mut reads = FileBroker::new([0x34; 32]);
    assert_eq!(
        reads.grant_file(
            &f.manager,
            &f.host,
            &instance,
            &binding,
            b"read".to_vec(),
            3
        ),
        Err(AdmissionError::Limit)
    );
    assert_eq!(binding.usage().resources, 1);
    targets
        .release(&f.manager, &f.host, &instance, selected.reference, 4)
        .unwrap();
    let token = reads
        .grant_file(
            &f.manager,
            &f.host,
            &instance,
            &binding,
            b"read".to_vec(),
            5,
        )
        .unwrap();
    assert_eq!(binding.usage().resources, 1);
    assert!(!token.is_empty());
    drop(reads);
    assert_eq!(binding.usage().resources, 0);
    instance.close(&mut f.host).unwrap();
}

#[test]
fn expired_or_stopped_selection_reaps_and_open_failure_releases_capacity() {
    for revoke in ["expiry", "stop"] {
        let mut f = Fixture::with_capabilities(true, 1, 1000, mutation_caps());
        let instance = f.connect();
        let binding = binding(&f, &instance, &mutation_caps());
        let path = f._dir.path().join("selected.bin");
        fs::write(&path, b"original").unwrap();
        let mut broker = TargetBroker::new([0x35; 32]).unwrap();
        let missing = f._dir.path().join("does-not-exist.bin");
        assert!(matches!(
            broker.select_existing(
                &f.manager,
                &f.host,
                &instance,
                &binding,
                &missing,
                scope(Disposition::Replace),
                || 2
            ),
            Err(TargetError::Io(std::io::ErrorKind::NotFound))
        ));
        assert_eq!(broker.len(), 0);
        assert_eq!(binding.usage().resources, 0);
        let selected = select(
            &mut broker,
            &f,
            &instance,
            &binding,
            &path,
            Disposition::Replace,
        );
        assert_eq!(binding.usage().resources, 1);
        if revoke == "stop" {
            instance.stop();
            broker.reap(3);
        } else {
            broker.reap(90);
        }
        assert_eq!(broker.len(), 0, "{revoke}");
        assert_eq!(binding.usage().resources, 0, "{revoke}");
        assert_eq!(fs::read(&path).unwrap(), b"original");
        assert_eq!(
            broker.release(&f.manager, &f.host, &instance, selected.reference, 91),
            Err(TargetError::Missing)
        );
        instance.close(&mut f.host).unwrap();
    }
}

#[test]
fn post_open_stop_or_expiry_cannot_publish_and_closes_handle() {
    for revoke in ["stop", "expiry"] {
        let mut f = Fixture::with_capabilities(true, 1, 1000, mutation_caps());
        let instance = f.connect();
        let binding = binding(&f, &instance, &mutation_caps());
        let path = f._dir.path().join("selected.bin");
        fs::write(&path, b"original").unwrap();
        let mut broker = TargetBroker::new([0x36; 32]).unwrap();
        let mut ticks = 0;
        let result = broker.select_existing(
            &f.manager,
            &f.host,
            &instance,
            &binding,
            &path,
            scope(Disposition::Replace),
            || {
                ticks += 1;
                if ticks == 5 {
                    if revoke == "stop" {
                        instance.stop();
                    } else {
                        return 90;
                    }
                }
                2
            },
        );
        assert_eq!(ticks, 5);
        assert_eq!(
            result,
            Err(TargetError::Admission(if revoke == "stop" {
                AdmissionError::Denied
            } else {
                AdmissionError::Expired
            }))
        );
        assert_eq!(broker.len(), 0);
        assert_eq!(binding.usage().resources, 0);
        assert_eq!(fs::read(&path).unwrap(), b"original");
        instance.close(&mut f.host).unwrap();
    }
}

#[test]
fn invalid_original_path_spellings_are_rejected_before_open_or_reservation() {
    let mut f = Fixture::with_capabilities(true, 1, 1000, mutation_caps());
    let instance = f.connect();
    let binding = binding(&f, &instance, &mutation_caps());
    let path = f._dir.path().join("selected.bin");
    fs::write(&path, b"original").unwrap();
    let base = f._dir.path().display().to_string();
    let paths = [
        format!("{base}\\selected.bin:stream"),
        format!("{base}\\.\\selected.bin"),
        format!("{base}\\..\\selected.bin"),
        format!("{base}\\CON.txt"),
        format!("{base}\\selected.bin."),
        format!("{base}\\selected.bin "),
        r"\\server\share\selected.bin".into(),
        r"\\.\C:\selected.bin".into(),
    ];
    let mut broker = TargetBroker::new([0x37; 32]).unwrap();
    for invalid in paths {
        let before = binding.usage();
        assert_eq!(
            broker.select_existing(
                &f.manager,
                &f.host,
                &instance,
                &binding,
                Path::new(&invalid),
                scope(Disposition::Replace),
                || 2
            ),
            Err(TargetError::InvalidSelection),
            "{invalid}"
        );
        assert_eq!(binding.usage(), before);
        assert_eq!(broker.len(), 0);
    }
    for subject in ["folder/name", "line\nbreak"] {
        let before = binding.usage();
        assert_eq!(
            broker.select_existing(
                &f.manager,
                &f.host,
                &instance,
                &binding,
                &path,
                SelectionScope {
                    subject: subject.into(),
                    approval_sha256: APPROVAL,
                    disposition: Disposition::Replace,
                },
                || 2,
            ),
            Err(TargetError::InvalidSelection),
            "{subject:?}"
        );
        assert_eq!(binding.usage(), before);
        assert_eq!(broker.len(), 0);
    }
    instance.close(&mut f.host).unwrap();
}

#[test]
fn final_symlink_is_rejected_when_windows_can_create_one() {
    let mut f = Fixture::with_capabilities(true, 1, 1000, mutation_caps());
    let instance = f.connect();
    let binding = binding(&f, &instance, &mutation_caps());
    let real = f._dir.path().join("real.bin");
    let link = f._dir.path().join("linked.bin");
    fs::write(&real, b"original").unwrap();
    if let Err(error) = std::os::windows::fs::symlink_file(&real, &link) {
        panic!("Windows symlink rejection unverified: symlink creation failed: {error}");
    }
    let mut broker = TargetBroker::new([0x38; 32]).unwrap();
    assert_eq!(
        broker.select_existing(
            &f.manager,
            &f.host,
            &instance,
            &binding,
            &link,
            scope(Disposition::Replace),
            || 2
        ),
        Err(TargetError::InvalidSelection)
    );
    assert_eq!(broker.len(), 0);
    assert_eq!(binding.usage().resources, 0);
    assert_eq!(fs::read(&real).unwrap(), b"original");
    instance.close(&mut f.host).unwrap();
}

const REPLACEMENT: &[u8] = b"future replacement bytes";

fn preparation_fixture() -> (
    Fixture,
    ManagedInstance,
    IoBinding,
    TargetBroker,
    RequestRecord,
    std::path::PathBuf,
) {
    let mut f = Fixture::with_capabilities(true, 2, 100_000, mutation_caps());
    let instance = f.connect();
    let binding = binding(&f, &instance, &mutation_caps());
    let path = f._dir.path().join("prepared-target.bin");
    fs::write(&path, b"original target bytes").unwrap();
    let mut broker = TargetBroker::new([0x39; 32]).unwrap();
    let selected = select(
        &mut broker,
        &f,
        &instance,
        &binding,
        &path,
        Disposition::Replace,
    );
    let request = plan(&f, selected, Disposition::Replace);
    (f, instance, binding, broker, request, path)
}

#[test]
fn preparation_and_staging_persist_exact_history_without_changing_selected_file() {
    use morrow_core::{file_content_receipt::Source, io_evidence::Kind, io_intent::Phase};

    let (mut f, instance, binding, broker, request, path) = preparation_fixture();
    let before_bytes = binding.usage().bytes;
    let prepared = broker
        .prepare_request(&f.manager, &mut f.host, &instance, &request, || 2)
        .unwrap();
    assert_eq!(prepared.phase(), Phase::Prepared);
    assert_eq!(f.host.store_local().pending(0, 10).unwrap().len(), 1);
    assert_eq!(
        f.host
            .store_local()
            .lookup_io_intent(SUBJECT, request.request().operation_id.as_str())
            .unwrap()
            .unwrap()
            .container(),
        prepared.container()
    );
    assert_eq!(
        f.host
            .store_local()
            .io_material(
                SUBJECT,
                request.request().operation_id.as_str(),
                Kind::Request
            )
            .unwrap()
            .unwrap()
            .payload(),
        request.container()
    );
    broker
        .stage_content(
            &f.manager,
            &mut f.host,
            &instance,
            &request,
            REPLACEMENT,
            || 3,
        )
        .unwrap();
    let store = f.host.store_local();
    assert_eq!(store.pending(0, 10).unwrap().len(), 2);
    assert_eq!(
        store
            .file_mutation_content_local_authorized(
                SUBJECT,
                request.request().operation_id.as_str(),
                || Ok(())
            )
            .unwrap()
            .unwrap()
            .content(),
        REPLACEMENT
    );
    let receipt = store
        .file_mutation_content_receipt_local_authorized(
            SUBJECT,
            request.request().operation_id.as_str(),
            || Ok(()),
        )
        .unwrap()
        .unwrap();
    assert_eq!(receipt.source(), Source::LiveStaging);
    assert_eq!(
        receipt.request_sha256(),
        request.command().unwrap().request_sha256
    );
    assert_eq!(
        receipt.content_sha256(),
        <[u8; 32]>::from(Sha256::digest(REPLACEMENT))
    );
    assert_eq!(
        binding.usage().bytes - before_bytes,
        request.container().len() as u64 + REPLACEMENT.len() as u64
    );
    assert_eq!(binding.usage().jobs, 0);
    assert_eq!(binding.usage().resources, 1);
    let pending = store.pending(0, 10).unwrap();
    broker
        .prepare_request(&f.manager, &mut f.host, &instance, &request, || 4)
        .unwrap();
    broker
        .stage_content(
            &f.manager,
            &mut f.host,
            &instance,
            &request,
            REPLACEMENT,
            || 5,
        )
        .unwrap();
    assert_eq!(f.host.store_local().pending(0, 10).unwrap(), pending);
    assert_eq!(
        binding.usage().bytes - before_bytes,
        2 * (request.container().len() as u64 + REPLACEMENT.len() as u64)
    );
    assert_eq!(fs::read(&path).unwrap_err().raw_os_error(), Some(32));
    assert!(matches!(
        f.host.store_local_mut().claim_io_dispatch_local_authorized(
            &prepared.propose_dispatch_boundary().unwrap(),
            || Ok(())
        ),
        Err(morrow_core::Error::UnsupportedVersion)
    ));
    drop(broker);
    assert_eq!(binding.usage().resources, 0);
    assert_eq!(fs::read(&path).unwrap(), b"original target bytes");
    let fresh = TargetBroker::new([0x39; 32]).unwrap();
    assert_eq!(
        fresh.validate_request(&f.manager, &f.host, &instance, &request, || 6),
        Err(TargetError::Missing)
    );
    assert_eq!(f.host.store_local().pending(0, 10).unwrap(), pending);
    f.host.store_local().integrity_check().unwrap();
    instance.close(&mut f.host).unwrap();
}

#[test]
fn mismatched_plan_action_or_stage_bytes_cannot_add_history() {
    let (mut f, instance, binding, broker, request, path) = preparation_fixture();
    let mut other = request.request().clone();
    other.subject = "another-subject".into();
    let other = RequestRecord::new(other).unwrap();
    assert!(matches!(
        broker.prepare_request(&f.manager, &mut f.host, &instance, &other, || 2),
        Err(TargetError::Mismatch)
    ));
    let mut delete = request.request().clone();
    delete.disposition = Disposition::Delete;
    delete.content_length = 0;
    delete.content_sha256 = None;
    let delete = RequestRecord::new(delete).unwrap();
    assert!(matches!(
        broker.prepare_request(&f.manager, &mut f.host, &instance, &delete, || 2),
        Err(TargetError::Mismatch)
    ));
    assert!(f.host.store_local().pending(0, 10).unwrap().is_empty());
    broker
        .prepare_request(&f.manager, &mut f.host, &instance, &request, || 2)
        .unwrap();
    let pending = f.host.store_local().pending(0, 10).unwrap();
    assert_eq!(pending.len(), 1);
    assert_eq!(
        broker.stage_content(&f.manager, &mut f.host, &instance, &request, b"short", || 3),
        Err(TargetError::Mismatch)
    );
    let same_length_wrong_hash = vec![b'x'; REPLACEMENT.len()];
    assert_eq!(
        broker.stage_content(
            &f.manager,
            &mut f.host,
            &instance,
            &request,
            &same_length_wrong_hash,
            || 3
        ),
        Err(TargetError::Mismatch)
    );
    assert_eq!(
        broker.stage_content(&f.manager, &mut f.host, &instance, &delete, b"", || 3),
        Err(TargetError::Mismatch)
    );
    assert_eq!(f.host.store_local().pending(0, 10).unwrap(), pending);
    assert_eq!(
        f.host.store_local().file_mutation_content_usage().unwrap(),
        (0, 0)
    );
    assert_eq!(fs::read(&path).unwrap_err().raw_os_error(), Some(32));
    assert_eq!(binding.usage().resources, 1);
    drop(broker);
    assert_eq!(fs::read(&path).unwrap(), b"original target bytes");
    instance.close(&mut f.host).unwrap();
}

fn successful_clock_samples(stage: bool) -> usize {
    let (mut f, instance, _binding, broker, request, _path) = preparation_fixture();
    if stage {
        broker
            .prepare_request(&f.manager, &mut f.host, &instance, &request, || 2)
            .unwrap();
    }
    let mut samples = 0;
    if stage {
        broker
            .stage_content(
                &f.manager,
                &mut f.host,
                &instance,
                &request,
                REPLACEMENT,
                || {
                    samples += 1;
                    3
                },
            )
            .unwrap();
    } else {
        broker
            .prepare_request(&f.manager, &mut f.host, &instance, &request, || {
                samples += 1;
                2
            })
            .unwrap();
    }
    assert!(samples >= 6);
    samples
}

#[test]
fn final_store_guard_rolls_back_but_postcommit_denial_keeps_history() {
    for stage in [false, true] {
        let samples = successful_clock_samples(stage);
        for after_commit in [false, true] {
            for revoke in ["stop", "expiry"] {
                let (mut f, instance, binding, mut broker, request, path) = preparation_fixture();
                if stage {
                    broker
                        .prepare_request(&f.manager, &mut f.host, &instance, &request, || 2)
                        .unwrap();
                }
                let baseline = f.host.store_local().pending(0, 10).unwrap();
                let trigger = samples - usize::from(!after_commit);
                let mut ticks = 0;
                let mut clock = || {
                    ticks += 1;
                    if ticks == trigger {
                        if revoke == "stop" {
                            instance.stop();
                        } else {
                            return 90;
                        }
                    }
                    if stage { 3 } else { 2 }
                };
                let result: std::result::Result<(), TargetError> = if stage {
                    broker.stage_content(
                        &f.manager,
                        &mut f.host,
                        &instance,
                        &request,
                        REPLACEMENT,
                        &mut clock,
                    )
                } else {
                    broker
                        .prepare_request(&f.manager, &mut f.host, &instance, &request, &mut clock)
                        .map(|_| ())
                };
                assert_eq!(
                    ticks, trigger,
                    "stage={stage} postcommit={after_commit} {revoke}"
                );
                let admission = if revoke == "stop" {
                    AdmissionError::Denied
                } else {
                    AdmissionError::Expired
                };
                assert_eq!(
                    result,
                    Err(if after_commit {
                        TargetError::CommittedButDeliveryDenied(admission)
                    } else {
                        TargetError::Admission(admission)
                    }),
                    "stage={stage} postcommit={after_commit} {revoke}"
                );
                let pending = f.host.store_local().pending(0, 10).unwrap();
                assert_eq!(
                    pending.len(),
                    baseline.len() + usize::from(after_commit),
                    "stage={stage} postcommit={after_commit} {revoke}"
                );
                let durable = if stage {
                    f.host
                        .store_local()
                        .file_mutation_content_usage()
                        .unwrap()
                        .0
                        == 1
                } else {
                    f.host
                        .store_local()
                        .lookup_io_intent(SUBJECT, request.request().operation_id.as_str())
                        .unwrap()
                        .is_some()
                };
                assert_eq!(durable, after_commit);
                f.host.store_local().integrity_check().unwrap();
                broker.reap(if revoke == "expiry" { 90 } else { 3 });
                assert_eq!(broker.len(), 0);
                assert_eq!(binding.usage().resources, 0);
                assert_eq!(fs::read(&path).unwrap(), b"original target bytes");
                instance.close(&mut f.host).unwrap();
            }
        }
    }
}
