//! Windows Create selection retains parent directories and stages bytes; no file is created.
use super::*;
use morrow_core::{
    file_content_receipt::Source,
    file_mutation::{Disposition, MutationRequest, RequestRecord, Target},
    file_path::RelativeFilePath,
    io_intent::Phase,
};
use morrow_plugin_runtime::{
    file_target::{Error as TargetError, SelectedCreateTarget, SelectionScope, TargetBroker},
    io_binding::Error as AdmissionError,
};
use sha2::{Digest, Sha256};
use std::{
    fs,
    os::windows::{fs::OpenOptionsExt, process::CommandExt},
    path::{Path, PathBuf},
    process::Command,
};

const SUBJECT: &str = "plugin.create-target-test";
const OPERATION: &str = "create-selected-file";
const APPROVAL: [u8; 32] = [0xc1; 32];
const BYTES: &[u8] = b"staged create content";

fn caps() -> BTreeSet<IoCapability> {
    BTreeSet::from([IoCapability::FileCreate])
}

fn fixture(resources: u32) -> (Fixture, ManagedInstance, IoBinding, TargetBroker, PathBuf) {
    let mut f = Fixture::with_capabilities(true, resources, 100_000, caps());
    let instance = f.connect();
    let binding = f
        .manager
        .bind_io(
            &f.host,
            &instance,
            f.package.digest(),
            f.manager.revision(),
            &caps(),
            90,
            1,
        )
        .unwrap();
    let root = f._dir.path().join("selected-root");
    fs::create_dir(&root).unwrap();
    (
        f,
        instance,
        binding,
        TargetBroker::new([0xc3; 32]).unwrap(),
        root,
    )
}

fn scope() -> SelectionScope {
    SelectionScope {
        subject: SUBJECT.into(),
        approval_sha256: APPROVAL,
        disposition: Disposition::Create,
    }
}

fn request(
    f: &Fixture,
    target: SelectedCreateTarget,
    relative: &RelativeFilePath,
) -> RequestRecord {
    RequestRecord::new(MutationRequest {
        operation_id: OPERATION.into(),
        subject: SUBJECT.into(),
        package_sha256: f.package.digest(),
        approval_sha256: APPROVAL,
        target: Target {
            reference: target.reference,
            relative_path: Some(relative.clone()),
        },
        disposition: Disposition::Create,
        expected_identity: None,
        content_length: BYTES.len() as u64,
        content_sha256: Some(Sha256::digest(BYTES).into()),
    })
    .unwrap()
}

fn select(
    broker: &mut TargetBroker,
    f: &Fixture,
    instance: &ManagedInstance,
    binding: &IoBinding,
    root: &Path,
    relative: &RelativeFilePath,
) -> SelectedCreateTarget {
    broker
        .select_create(
            &f.manager,
            &f.host,
            instance,
            binding,
            root,
            relative,
            scope(),
            || 2,
        )
        .unwrap()
}

#[test]
fn nested_unicode_create_selection_stages_exact_bytes_without_creating_leaf() {
    let (mut f, instance, binding, mut broker, root) = fixture(4);
    fs::create_dir_all(root.join("子目录").join("second")).unwrap();
    let relative = RelativeFilePath::parse("子目录/second/新建.txt").unwrap();
    let leaf = root.join("子目录").join("second").join("新建.txt");
    let target = select(&mut broker, &f, &instance, &binding, &root, &relative);
    assert!(!leaf.exists());
    assert_eq!(binding.usage().resources, 3);
    let plan = request(&f, target, &relative);
    broker
        .validate_request(&f.manager, &f.host, &instance, &plan, || 3)
        .unwrap();
    let prepared = broker
        .prepare_request(&f.manager, &mut f.host, &instance, &plan, || 4)
        .unwrap();
    assert_eq!(prepared.phase(), Phase::Prepared);
    broker
        .stage_content(&f.manager, &mut f.host, &instance, &plan, BYTES, || 5)
        .unwrap();
    assert!(!leaf.exists());
    let content = f
        .host
        .store_local()
        .file_mutation_content_local_authorized(SUBJECT, OPERATION, || Ok(()))
        .unwrap()
        .unwrap();
    assert_eq!(content.content(), BYTES);
    let receipt = f
        .host
        .store_local()
        .file_mutation_content_receipt_local_authorized(SUBJECT, OPERATION, || Ok(()))
        .unwrap()
        .unwrap();
    assert_eq!(receipt.source(), Source::LiveStaging);
    assert_eq!(
        receipt.content_sha256(),
        <[u8; 32]>::from(Sha256::digest(BYTES))
    );
    assert_eq!(f.host.store_local().pending(0, 10).unwrap().len(), 2);
    broker
        .release(&f.manager, &f.host, &instance, target.reference, 6)
        .unwrap();
    assert_eq!(binding.usage().resources, 0);
    f.host.store_local().integrity_check().unwrap();
    instance.close(&mut f.host).unwrap();
}

#[test]
fn existing_leaf_is_never_opened_or_modified_during_create_preparation() {
    let (mut f, instance, binding, mut broker, root) = fixture(2);
    let relative = RelativeFilePath::parse("already.txt").unwrap();
    let leaf = root.join("already.txt");
    fs::write(&leaf, b"existing file remains").unwrap();
    let held = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .share_mode(0)
        .open(&leaf)
        .unwrap();
    let target = select(&mut broker, &f, &instance, &binding, &root, &relative);
    let plan = request(&f, target, &relative);
    broker
        .prepare_request(&f.manager, &mut f.host, &instance, &plan, || 3)
        .unwrap();
    broker
        .stage_content(&f.manager, &mut f.host, &instance, &plan, BYTES, || 4)
        .unwrap();
    drop(held);
    assert_eq!(fs::read(&leaf).unwrap(), b"existing file remains");
    broker
        .release(&f.manager, &f.host, &instance, target.reference, 5)
        .unwrap();
    instance.close(&mut f.host).unwrap();
}

#[test]
fn missing_parent_and_file_as_parent_fail_without_retained_resources() {
    let (mut f, instance, binding, mut broker, root) = fixture(4);
    fs::write(root.join("ordinary-file"), b"not a directory").unwrap();
    for relative in ["missing/leaf.txt", "ordinary-file/leaf.txt"] {
        let relative = RelativeFilePath::parse(relative).unwrap();
        let result = broker.select_create(
            &f.manager,
            &f.host,
            &instance,
            &binding,
            &root,
            &relative,
            scope(),
            || 2,
        );
        assert!(result.is_err());
        assert_eq!(broker.len(), 0);
        assert_eq!(binding.usage().resources, 0);
    }
    assert_eq!(
        fs::read(root.join("ordinary-file")).unwrap(),
        b"not a directory"
    );
    instance.close(&mut f.host).unwrap();
}

#[test]
fn resource_quota_failure_before_opening_parents_releases_every_slot() {
    let (mut f, instance, binding, mut broker, root) = fixture(2);
    fs::create_dir_all(root.join("one").join("two")).unwrap();
    // An OS open would fail due to this writer. Limit must take precedence:
    // the entire resource chain is reserved before touching the filesystem.
    let held = fs::OpenOptions::new()
        .access_mode(0x40000000)
        .share_mode(7)
        .custom_flags(0x02000000)
        .open(&root)
        .unwrap();
    let relative = RelativeFilePath::parse("one/two/new.txt").unwrap();
    let result = broker.select_create(
        &f.manager,
        &f.host,
        &instance,
        &binding,
        &root,
        &relative,
        scope(),
        || 2,
    );
    assert!(matches!(
        result,
        Err(TargetError::Admission(AdmissionError::Limit)) | Err(TargetError::Limit)
    ));
    assert_eq!(broker.len(), 0);
    assert_eq!(binding.usage().resources, 0);
    assert!(!root.join("one/two/new.txt").exists());
    drop(held);
    instance.close(&mut f.host).unwrap();
}

#[test]
fn exact_relative_scope_package_and_live_owner_are_required() {
    let (mut f, instance, binding, mut broker, root) = fixture(3);
    fs::create_dir(root.join("parent")).unwrap();
    let relative = RelativeFilePath::parse("parent/leaf.txt").unwrap();
    let target = select(&mut broker, &f, &instance, &binding, &root, &relative);
    let plan = request(&f, target, &relative);
    let foreign = f.connect();
    assert!(matches!(
        broker.validate_request(&f.manager, &f.host, &foreign, &plan, || 3),
        Err(TargetError::Admission(AdmissionError::Denied))
    ));
    for variant in [
        "relative",
        "subject",
        "package",
        "approval",
        "action",
        "identity",
        "reference",
    ] {
        let mut changed = plan.request().clone();
        match variant {
            "relative" => {
                changed.target.relative_path =
                    Some(RelativeFilePath::parse("parent/other.txt").unwrap())
            }
            "subject" => changed.subject = "other-subject".into(),
            "package" => changed.package_sha256 = [7; 32],
            "approval" => changed.approval_sha256 = [8; 32],
            "action" => {
                changed.disposition = Disposition::Replace;
                changed.expected_identity = Some([9; 32]);
            }
            "identity" => changed.expected_identity = Some([9; 32]),
            "reference" => changed.target.reference = [6; 32],
            _ => unreachable!(),
        }
        if variant == "identity" {
            assert!(RequestRecord::new(changed).is_err());
            continue;
        }
        let changed = RequestRecord::new(changed).unwrap();
        let expected = if variant == "reference" {
            TargetError::Missing
        } else {
            TargetError::Mismatch
        };
        assert_eq!(
            broker.validate_request(&f.manager, &f.host, &instance, &changed, || 3),
            Err(expected),
            "{variant}"
        );
    }
    assert!(f.host.store_local().pending(0, 10).unwrap().is_empty());
    foreign.close(&mut f.host).unwrap();
    broker
        .release(&f.manager, &f.host, &instance, target.reference, 4)
        .unwrap();
    instance.close(&mut f.host).unwrap();
}

#[test]
fn retained_root_and_parent_block_rename_until_release() {
    let (mut f, instance, binding, mut broker, root) = fixture(3);
    let parent = root.join("parent");
    fs::create_dir(&parent).unwrap();
    let relative = RelativeFilePath::parse("parent/leaf.txt").unwrap();
    let target = select(&mut broker, &f, &instance, &binding, &root, &relative);
    assert_eq!(binding.usage().resources, 2);
    let moved_root = f._dir.path().join("moved-root");
    let moved_parent = root.join("moved-parent");
    assert!(fs::rename(&root, &moved_root).is_err());
    assert!(fs::rename(&parent, &moved_parent).is_err());
    broker
        .release(&f.manager, &f.host, &instance, target.reference, 3)
        .unwrap();
    assert_eq!(binding.usage().resources, 0);
    fs::rename(&parent, &moved_parent).unwrap();
    fs::rename(&root, &moved_root).unwrap();
    fs::rename(&moved_root, &root).unwrap();
    instance.close(&mut f.host).unwrap();
}

#[test]
fn expiry_or_stop_reap_releases_all_ancestor_handles() {
    for revoke in ["expiry", "stop"] {
        let (mut f, instance, binding, mut broker, root) = fixture(3);
        fs::create_dir(root.join("parent")).unwrap();
        let relative = RelativeFilePath::parse("parent/leaf.txt").unwrap();
        let target = select(&mut broker, &f, &instance, &binding, &root, &relative);
        assert_ne!(target.reference, [0; 32]);
        assert_eq!(binding.usage().resources, 2);
        if revoke == "stop" {
            instance.stop();
            broker.reap(3);
        } else {
            broker.reap(90);
        }
        assert_eq!(broker.len(), 0);
        assert_eq!(binding.usage().resources, 0);
        fs::rename(root.join("parent"), root.join("renamed")).unwrap();
        instance.close(&mut f.host).unwrap();
    }
}

fn junction(link: &Path, target: &Path) {
    // Only fixture-generated paths are used. raw_arg avoids Command's C-runtime
    // quoting, which cmd builtins do not understand. Disable auto-run/delayed
    // expansion and reject the remaining expansion/control characters first.
    for path in [link, target] {
        assert!(
            !path
                .to_str()
                .unwrap()
                .chars()
                .any(|c| matches!(c, '"' | '%' | '\r' | '\n')),
            "temporary junction path contains command expansion characters"
        );
    }
    let command = format!("mklink /J \"{}\" \"{}\"", link.display(), target.display());
    let result = Command::new("cmd")
        .args(["/D", "/V:OFF", "/C"])
        .raw_arg(command)
        .creation_flags(0x08000000)
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "mklink /J failed: {}",
        String::from_utf8_lossy(&result.stderr)
    );
}

#[test]
fn root_or_intermediate_junction_is_rejected_without_reservation() {
    let (mut f, instance, binding, mut broker, root) = fixture(4);
    let real = f._dir.path().join("real-directory");
    fs::create_dir(&real).unwrap();
    let root_link = f._dir.path().join("root-junction");
    junction(&root_link, &root);
    let direct = RelativeFilePath::parse("new.txt").unwrap();
    assert!(
        broker
            .select_create(
                &f.manager,
                &f.host,
                &instance,
                &binding,
                &root_link,
                &direct,
                scope(),
                || 2
            )
            .is_err()
    );
    let middle = root.join("middle-junction");
    junction(&middle, &real);
    let nested = RelativeFilePath::parse("middle-junction/new.txt").unwrap();
    assert!(
        broker
            .select_create(
                &f.manager,
                &f.host,
                &instance,
                &binding,
                &root,
                &nested,
                scope(),
                || 2
            )
            .is_err()
    );
    assert_eq!(broker.len(), 0);
    assert_eq!(binding.usage().resources, 0);
    assert!(!real.join("new.txt").exists());
    fs::remove_dir(&middle).unwrap();
    fs::remove_dir(&root_link).unwrap();
    instance.close(&mut f.host).unwrap();
}

#[test]
fn authority_lost_after_opening_last_parent_closes_entire_unpublished_selection() {
    for reason in ["stop", "expiry"] {
        let (mut f, instance, binding, mut broker, root) = fixture(3);
        fs::create_dir(root.join("parent")).unwrap();
        let relative = RelativeFilePath::parse("parent/leaf.txt").unwrap();
        let mut calls = 0;
        let result = broker.select_create(
            &f.manager,
            &f.host,
            &instance,
            &binding,
            &root,
            &relative,
            scope(),
            || {
                calls += 1;
                if calls == 8 {
                    if reason == "stop" {
                        instance.stop();
                    } else {
                        return 90;
                    }
                }
                2
            },
        );
        assert_eq!(calls, 8, "exercise post-open delivery guard");
        assert_eq!(
            result,
            Err(TargetError::Admission(if reason == "stop" {
                AdmissionError::Denied
            } else {
                AdmissionError::Expired
            }))
        );
        assert!(broker.is_empty());
        assert_eq!(binding.usage().resources, 0);
        fs::rename(root.join("parent"), root.join("renamed")).unwrap();
        assert!(f.host.store_local().pending(0, 10).unwrap().is_empty());
        instance.close(&mut f.host).unwrap();
    }
}

#[test]
fn fresh_selection_cannot_restore_old_prepared_create_authority() {
    let (mut f, instance, binding, mut broker, root) = fixture(2);
    let relative = RelativeFilePath::parse("new.txt").unwrap();
    let old = select(&mut broker, &f, &instance, &binding, &root, &relative);
    let plan = request(&f, old, &relative);
    broker
        .prepare_request(&f.manager, &mut f.host, &instance, &plan, || 3)
        .unwrap();
    drop(broker);
    assert_eq!(binding.usage().resources, 0);
    let mut broker = TargetBroker::new([0xc3; 32]).unwrap();
    let fresh = broker
        .select_create(
            &f.manager,
            &f.host,
            &instance,
            &binding,
            &root,
            &relative,
            scope(),
            || 4,
        )
        .unwrap();
    assert_ne!(fresh.reference, old.reference);
    assert_eq!(
        broker.stage_content(&f.manager, &mut f.host, &instance, &plan, BYTES, || 5),
        Err(TargetError::Missing)
    );
    assert_eq!(f.host.store_local().pending(0, 10).unwrap().len(), 1);
    assert!(!root.join("new.txt").exists());
    broker
        .release(&f.manager, &f.host, &instance, fresh.reference, 6)
        .unwrap();
    instance.close(&mut f.host).unwrap();
}
