//! Windows Replace must fail closed until the native effect has an atomic target precondition.
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
    let mut f = Fixture::with_capabilities(true, 2, 100_000, caps.clone());
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
fn real_selected_replace_repeatedly_fails_closed_without_claim_or_file_effect() {
    let (mut f, instance, binding, mut broker, request, target, alias) = setup(true);
    let usage = binding.usage();
    let pending = f.host.store_local().pending(0, 10).unwrap();
    for tick in [5, 6, 7] {
        assert!(matches!(
            broker.replace(&f.manager, &mut f.host, &instance, &request, || tick),
            Err(ReplaceError::UnsupportedConditionalReplacement)
        ));
        assert_eq!(binding.usage(), usage);
        assert_eq!(f.host.store_local().pending(0, 10).unwrap(), pending);
        prepared(&f, &request, true);
        assert_eq!(fs::read(&target).unwrap_err().raw_os_error(), Some(32));
    }
    let names: Vec<_> = fs::read_dir(f._dir.path())
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    assert!(
        !names
            .iter()
            .any(|name| name.starts_with(".morrow-") && name.ends_with(".tmp"))
    );
    assert_eq!(broker.len(), 1);
    assert_eq!(binding.usage().resources, 1);
    broker
        .release(
            &f.manager,
            &f.host,
            &instance,
            request.request().target.reference,
            8,
        )
        .unwrap();
    assert_eq!(binding.usage().resources, 0);
    assert_eq!(fs::read(&target).unwrap(), ORIGINAL);
    assert_eq!(fs::read(&alias).unwrap(), ORIGINAL);
    instance.close(&mut f.host).unwrap();
}

#[test]
fn unsupported_replace_does_not_require_staged_bytes_or_charge_execution_budget() {
    let (mut f, instance, binding, mut broker, request, target, alias) = setup(false);
    let usage = binding.usage();
    assert!(matches!(
        broker.replace(&f.manager, &mut f.host, &instance, &request, || 4),
        Err(ReplaceError::UnsupportedConditionalReplacement)
    ));
    assert_eq!(binding.usage(), usage);
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
    assert_eq!(fs::read(&target).unwrap(), ORIGINAL);
    assert_eq!(fs::read(&alias).unwrap(), ORIGINAL);
    instance.close(&mut f.host).unwrap();
}

#[test]
fn foreign_owner_or_wrong_plan_is_rejected_before_support_status_is_revealed() {
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
