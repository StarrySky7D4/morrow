//! Actual Windows transactions on new synthetic files; no account/policy setup.
use super::*;
use std::{
    fs::{self, OpenOptions},
    os::windows::fs::OpenOptionsExt,
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
};

static NEXT: AtomicUsize = AtomicUsize::new(1);
struct Charge(Arc<AtomicUsize>);
impl Drop for Charge {
    fn drop(&mut self) {
        self.0.fetch_add(1, Ordering::SeqCst);
    }
}
fn charge() -> (Charge, Arc<AtomicUsize>) {
    let observed = Arc::new(AtomicUsize::new(0));
    (Charge(observed.clone()), observed)
}
fn fixture() -> (PathBuf, PathBuf) {
    let root = PathBuf::from(
        std::env::var_os("MORROW_TXF_TEST_ROOT")
            .expect("explicit fresh synthetic fixture root required"),
    );
    assert!(root.is_absolute());
    let directory = root.join(format!(
        "{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::SeqCst)
    ));
    fs::create_dir(&directory).expect("each test owns a new, absent directory");
    let target = directory.join("original.bin");
    fs::write(&target, b"original synthetic bytes").unwrap();
    (directory, target)
}
fn begin(charge: Charge) -> Transaction<Charge> {
    Transaction::begin(Duration::from_secs(5), 1, charge).unwrap()
}
fn writable(path: &Path) -> bool {
    OpenOptions::new()
        .write(true)
        .share_mode(7)
        .open(path)
        .is_ok()
}
fn selected(path: &Path) -> File {
    OpenOptions::new()
        .read(true)
        .share_mode(7)
        .open(path)
        .unwrap()
}

#[test]
fn invalid_lifetime_does_not_retain_a_reservation() {
    for duration in [
        Duration::ZERO,
        Duration::from_nanos(1),
        Duration::from_millis(u64::from(u32::MAX) + 1),
    ] {
        let (charge, observed) = charge();
        assert!(matches!(
            Transaction::begin(duration, 1, charge),
            Err(Failure::InvalidBounds)
        ));
        assert_eq!(observed.load(Ordering::SeqCst), 1);
    }
}

#[test]
#[ignore = "requires an explicit disposable Windows fixture root"]
fn transaction_retains_writer_condition_after_transaction_file_closes() {
    let (directory, target) = fixture();
    let selection = selected(&target);
    let alias = directory.join("original-alias.bin");
    fs::hard_link(&target, &alias).unwrap();
    let competitor = directory.join("competitor.bin");
    fs::write(&competitor, b"competitor").unwrap();
    let (charge, observed) = charge();
    let mut transaction = begin(charge);
    assert_eq!(
        transaction
            .retain_writer(&target, &selection)
            .unwrap()
            .len(),
        24
    );
    assert!(!writable(&target));
    transaction.close_files().unwrap();
    assert_eq!(fs::read(&target).unwrap(), b"original synthetic bytes");
    assert!(!writable(&target));
    assert!(!writable(&alias));
    assert!(fs::rename(&competitor, &target).is_err());
    assert!(fs::remove_file(&target).is_err());
    assert_eq!(observed.load(Ordering::SeqCst), 0);
    transaction.rollback().unwrap();
    assert_eq!(observed.load(Ordering::SeqCst), 1);
    assert!(writable(&target));
    drop(selection);
    fs::rename(&competitor, &target).unwrap();
    assert_eq!(fs::read(target).unwrap(), b"competitor");
    assert_eq!(fs::read(alias).unwrap(), b"original synthetic bytes");
}

#[test]
#[ignore = "requires an explicit disposable Windows fixture root"]
fn successful_completion_closes_before_returning_the_original_charge() {
    let (_, target) = fixture();
    let selection = selected(&target);
    let (charge, observed) = charge();
    let mut transaction = begin(charge);
    transaction.retain_writer(&target, &selection).unwrap();
    let calls = AtomicUsize::new(0);
    transaction
        .commit(|| {
            calls.fetch_add(1, Ordering::SeqCst);
            true
        })
        .unwrap();
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert_eq!(observed.load(Ordering::SeqCst), 1);
    assert!(writable(&target));
    assert_eq!(fs::read(target).unwrap(), b"original synthetic bytes");
}

#[test]
#[ignore = "requires an explicit disposable Windows fixture root"]
fn revoked_final_authority_rolls_back_without_commit_or_replay() {
    let (_, target) = fixture();
    let selection = selected(&target);
    let (charge, observed) = charge();
    let mut transaction = begin(charge);
    transaction.retain_writer(&target, &selection).unwrap();
    assert_eq!(transaction.commit(|| false), Err(Failure::Inactive));
    assert_eq!(observed.load(Ordering::SeqCst), 1);
    assert!(writable(&target));
    assert_eq!(fs::read(target).unwrap(), b"original synthetic bytes");
}

#[test]
#[ignore = "requires an explicit disposable Windows fixture root"]
fn prior_writer_is_rejected_without_mutating_the_selected_file() {
    let (_, target) = fixture();
    let selection = selected(&target);
    let previous = OpenOptions::new()
        .read(true)
        .write(true)
        .share_mode(7)
        .open(&target)
        .unwrap();
    let (charge, observed) = charge();
    let mut transaction = begin(charge);
    assert!(matches!(
        transaction.retain_writer(&target, &selection),
        Err(Failure::Os(32 | 6800))
    ));
    transaction.rollback().unwrap();
    assert_eq!(observed.load(Ordering::SeqCst), 1);
    drop(previous);
    assert_eq!(fs::read(target).unwrap(), b"original synthetic bytes");
}

#[test]
#[ignore = "requires an explicit disposable Windows fixture root"]
fn a_native_file_open_cannot_exceed_its_reserved_capacity() {
    let (directory, target) = fixture();
    let selection = selected(&target);
    let second = directory.join("not-selected.bin");
    fs::write(&second, b"other").unwrap();
    let second_selection = selected(&second);
    let (charge, observed) = charge();
    let mut transaction = begin(charge);
    transaction.retain_writer(&target, &selection).unwrap();
    assert!(matches!(
        transaction.retain_writer(&second, &second_selection),
        Err(Failure::Limit)
    ));
    assert!(writable(&second));
    transaction.rollback().unwrap();
    assert_eq!(observed.load(Ordering::SeqCst), 1);
}

#[test]
#[ignore = "requires an explicit disposable Windows fixture root"]
fn unwinding_original_owner_rolls_back_and_closes_once() {
    let (_, target) = fixture();
    let selection = selected(&target);
    let (charge, observed) = charge();
    let mut transaction = begin(charge);
    transaction.retain_writer(&target, &selection).unwrap();
    let failure = std::panic::catch_unwind(std::panic::AssertUnwindSafe(move || {
        let _original_owner = transaction;
        panic!("synthetic original-owner unwind");
    }));
    assert!(failure.is_err());
    assert_eq!(observed.load(Ordering::SeqCst), 1);
    assert!(writable(&target));
    assert_eq!(fs::read(target).unwrap(), b"original synthetic bytes");
}

#[test]
#[ignore = "requires an explicit disposable Windows fixture root"]
fn expired_owner_cannot_renew_itself_by_calling_commit() {
    let (_, target) = fixture();
    let selection = selected(&target);
    let (charge, observed) = charge();
    let mut transaction = Transaction::begin(Duration::from_millis(250), 1, charge).unwrap();
    transaction.retain_writer(&target, &selection).unwrap();
    std::thread::sleep(Duration::from_millis(400));
    let calls = AtomicUsize::new(0);
    assert_eq!(
        transaction.commit(|| {
            calls.fetch_add(1, Ordering::SeqCst);
            true
        }),
        Err(Failure::Inactive)
    );
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    // Unknown rollback deliberately retains its charge; known rollback releases
    // it. This test reports the native behavior instead of weakening the rule.
    println!(
        "EXPIRED_TRANSACTION_RELEASED_CHARGE={}",
        observed.load(Ordering::SeqCst)
    );
    assert!(writable(&target));
    assert_eq!(fs::read(target).unwrap(), b"original synthetic bytes");
}

#[test]
#[ignore = "requires an explicit disposable Windows fixture root"]
fn expiry_during_final_authorization_is_checked_again_before_commit() {
    let (_, target) = fixture();
    let selection = selected(&target);
    let (charge, observed) = charge();
    let mut transaction = Transaction::begin(Duration::from_millis(250), 1, charge).unwrap();
    transaction.retain_writer(&target, &selection).unwrap();
    assert_eq!(
        transaction.commit(|| {
            std::thread::sleep(Duration::from_millis(400));
            true
        }),
        Err(Failure::Inactive)
    );
    println!(
        "EXPIRED_AFTER_AUTH_RELEASED_CHARGE={}",
        observed.load(Ordering::SeqCst)
    );
    assert!(writable(&target));
    assert_eq!(fs::read(target).unwrap(), b"original synthetic bytes");
}

#[test]
#[ignore = "requires an explicit disposable Windows fixture root"]
fn same_bytes_at_replaced_path_cannot_commit_for_the_original_selection() {
    let (directory, target) = fixture();
    let selection = selected(&target);
    let displaced = directory.join("displaced-original.bin");
    fs::rename(&target, &displaced).unwrap();
    fs::write(&target, b"original synthetic bytes").unwrap();
    let (charge, observed) = charge();
    let mut transaction = begin(charge);
    assert!(matches!(
        transaction.retain_writer(&target, &selection),
        Err(Failure::SelectionMismatch)
    ));
    let calls = AtomicUsize::new(0);
    assert_eq!(
        transaction.commit(|| {
            calls.fetch_add(1, Ordering::SeqCst);
            true
        }),
        Err(Failure::SelectionMismatch)
    );
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    assert_eq!(observed.load(Ordering::SeqCst), 1);
    assert!(writable(&target));
    assert!(writable(&displaced));
    assert_eq!(fs::read(target).unwrap(), b"original synthetic bytes");
    assert_eq!(fs::read(displaced).unwrap(), b"original synthetic bytes");
}

#[test]
#[ignore = "requires an explicit disposable Windows fixture root"]
fn failed_selection_cannot_be_replaced_after_closing_transaction_files() {
    let (directory, target) = fixture();
    let selection = selected(&target);
    let displaced = directory.join("displaced-original.bin");
    fs::rename(&target, &displaced).unwrap();
    fs::write(&target, b"competitor bytes").unwrap();
    let competitor = selected(&target);
    let (charge, observed) = charge();
    let mut transaction = begin(charge);
    assert!(matches!(
        transaction.retain_writer(&target, &selection),
        Err(Failure::SelectionMismatch)
    ));
    transaction.close_files().unwrap();
    assert!(matches!(
        transaction.retain_writer(&target, &competitor),
        Err(Failure::SelectionMismatch)
    ));
    let calls = AtomicUsize::new(0);
    assert_eq!(
        transaction.commit(|| {
            calls.fetch_add(1, Ordering::SeqCst);
            true
        }),
        Err(Failure::SelectionMismatch)
    );
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    assert_eq!(observed.load(Ordering::SeqCst), 1);
    assert!(writable(&target));
    assert_eq!(fs::read(target).unwrap(), b"competitor bytes");
    assert_eq!(fs::read(displaced).unwrap(), b"original synthetic bytes");
}

#[test]
#[ignore = "requires an explicit disposable Windows fixture root"]
fn file_closure_cannot_reopen_acquisition_or_reuse_a_retained_lock_reservation() {
    for lock_first in [false, true] {
        let (directory, target) = fixture();
        let selection = selected(&target);
        let second = directory.join("second.bin");
        fs::write(&second, b"second synthetic bytes").unwrap();
        let second_selection = selected(&second);
        let (charge, observed) = charge();
        let mut transaction = begin(charge);
        if lock_first {
            transaction.retain_writer(&target, &selection).unwrap();
        }
        transaction.close_files().unwrap();
        assert_eq!(writable(&target), !lock_first);
        assert!(matches!(
            transaction.retain_writer(&second, &second_selection),
            Err(Failure::Inactive)
        ));
        assert!(writable(&second));
        assert_eq!(observed.load(Ordering::SeqCst), 0);
        let calls = AtomicUsize::new(0);
        assert_eq!(
            transaction.commit(|| {
                calls.fetch_add(1, Ordering::SeqCst);
                true
            }),
            Err(Failure::Inactive)
        );
        assert_eq!(calls.load(Ordering::SeqCst), 0);
        assert_eq!(observed.load(Ordering::SeqCst), 1);
        assert!(writable(&target));
        assert!(writable(&second));
        assert_eq!(fs::read(target).unwrap(), b"original synthetic bytes");
        assert_eq!(fs::read(second).unwrap(), b"second synthetic bytes");
    }
}

type MoveProgress =
    unsafe extern "system" fn(i64, i64, i64, i64, u32, u32, Handle, Handle, Handle) -> u32;

#[link(name = "kernel32")]
unsafe extern "system" {
    fn MoveFileTransactedW(
        source: *const u16,
        destination: *const u16,
        progress: Option<MoveProgress>,
        data: Handle,
        flags: u32,
        transaction: Handle,
    ) -> i32;
}

// Backend feasibility only. The public broker remains Unsupported: this does
// not supply its ancestry, Store claim, metadata, quota, or crash reconciliation.
fn replace_synthetic_file_under_original_transaction(commit: bool) {
    let (directory, target) = fixture();
    let selection = selected(&target);
    let original_identity = identity(&selection).unwrap();
    let source = directory.join("replacement.bin");
    let competitor = directory.join("competitor.bin");
    fs::write(&source, b"replacement synthetic bytes").unwrap();
    fs::write(&competitor, b"competing synthetic bytes").unwrap();
    let (charge, observed) = charge();
    let mut transaction = begin(charge);
    transaction.retain_writer(&target, &selection).unwrap();
    // Close the read-only selection only after the original native writer
    // condition is held. All file handles must close before a TxF deletion.
    drop(selection);
    transaction.close_files().unwrap();
    assert!(!writable(&target));
    assert!(fs::rename(&competitor, &target).is_err());
    transaction.live().unwrap();
    let wide_source: Vec<u16> = source.as_os_str().encode_wide().chain(Some(0)).collect();
    let wide_target: Vec<u16> = target.as_os_str().encode_wide().chain(Some(0)).collect();
    // SAFETY: both terminated names are new files in this test's exclusive
    // synthetic directory. The original transaction is borrowed and live.
    // Only REPLACE_EXISTING is used: no copying across volumes or reboot work.
    let moved = unsafe {
        MoveFileTransactedW(
            wide_source.as_ptr(),
            wide_target.as_ptr(),
            None,
            std::ptr::null_mut(),
            1,
            transaction.handle.unwrap(),
        )
    };
    assert_ne!(moved, 0, "native transacted move failed: {}", last_error());
    assert_eq!(fs::read(&target).unwrap(), b"original synthetic bytes");
    assert_eq!(fs::read(&source).unwrap(), b"replacement synthetic bytes");
    assert!(fs::rename(&competitor, &target).is_err());
    assert!(!writable(&target));
    assert_eq!(observed.load(Ordering::SeqCst), 0);
    let calls = AtomicUsize::new(0);
    let outcome = transaction.commit(|| {
        calls.fetch_add(1, Ordering::SeqCst);
        commit
    });
    assert_eq!(
        outcome,
        if commit {
            Ok(())
        } else {
            Err(Failure::Inactive)
        }
    );
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert_eq!(observed.load(Ordering::SeqCst), 1);
    assert!(writable(&target));
    assert_eq!(fs::read(&competitor).unwrap(), b"competing synthetic bytes");
    let actual = identity(&selected(&target)).unwrap();
    if commit {
        assert_eq!(fs::read(&target).unwrap(), b"replacement synthetic bytes");
        assert!(!source.exists());
        assert!(actual != original_identity);
    } else {
        assert_eq!(fs::read(&target).unwrap(), b"original synthetic bytes");
        assert_eq!(fs::read(&source).unwrap(), b"replacement synthetic bytes");
        assert!(actual == original_identity);
    }
}

#[test]
#[ignore = "requires an explicit disposable Windows fixture root"]
fn transacted_replacement_remains_isolated_until_original_owner_commits() {
    replace_synthetic_file_under_original_transaction(true);
}

#[test]
#[ignore = "requires an explicit disposable Windows fixture root"]
fn revoked_replacement_commit_restores_both_names_and_original_bytes() {
    replace_synthetic_file_under_original_transaction(false);
}

#[test]
#[ignore = "requires an explicit disposable Windows fixture root"]
fn production_staging_commits_exact_empty_and_binary_content_without_changing_original_alias() {
    for content in [b"".as_slice(), b"\0binary\r\n\xffpayload  ".as_slice()] {
        let (directory, target) = fixture();
        let selection = selected(&target);
        let original_identity = identity(&selection).unwrap();
        let alias = directory.join("original-alias.bin");
        fs::hard_link(&target, &alias).unwrap();
        let (charge, observed) = charge();
        let mut transaction = Transaction::begin(Duration::from_secs(5), 2, charge).unwrap();
        transaction.retain_writer(&target, &selection).unwrap();
        drop(selection);
        transaction
            .stage_replacement("owned-new.tmp", content, || true)
            .unwrap();
        assert_eq!(fs::read(&target).unwrap(), b"original synthetic bytes");
        assert!(!writable(&target));
        assert_eq!(observed.load(Ordering::SeqCst), 0);
        transaction.commit(|| true).unwrap();
        assert_eq!(fs::read(&target).unwrap(), content);
        assert_eq!(fs::read(&alias).unwrap(), b"original synthetic bytes");
        assert!(!directory.join("owned-new.tmp").exists());
        assert!(identity(&selected(&target)).unwrap() != original_identity);
        assert_eq!(observed.load(Ordering::SeqCst), 1);
    }
}

#[test]
#[ignore = "requires an explicit disposable Windows fixture root"]
fn production_staging_revoked_commit_discards_temporary_and_preserves_target() {
    let (directory, target) = fixture();
    let selection = selected(&target);
    let original_identity = identity(&selection).unwrap();
    let (charge, observed) = charge();
    let mut transaction = Transaction::begin(Duration::from_secs(5), 2, charge).unwrap();
    transaction.retain_writer(&target, &selection).unwrap();
    drop(selection);
    transaction
        .stage_replacement("owned-new.tmp", b"replacement", || true)
        .unwrap();
    assert_eq!(transaction.commit(|| false), Err(Failure::Inactive));
    assert_eq!(fs::read(&target).unwrap(), b"original synthetic bytes");
    assert!(identity(&selected(&target)).unwrap() == original_identity);
    assert!(!directory.join("owned-new.tmp").exists());
    assert_eq!(observed.load(Ordering::SeqCst), 1);
}

#[test]
#[ignore = "requires an explicit disposable Windows fixture root"]
fn production_staging_cancellation_after_partial_write_is_sticky_and_rolls_back() {
    let (directory, target) = fixture();
    let selection = selected(&target);
    let (charge, observed) = charge();
    let mut transaction = Transaction::begin(Duration::from_secs(5), 2, charge).unwrap();
    transaction.retain_writer(&target, &selection).unwrap();
    drop(selection);
    let mut calls = 0;
    assert_eq!(
        transaction.stage_replacement("owned-new.tmp", &vec![0x5a; 131_079], || {
            calls += 1;
            calls < 3
        }),
        Err(Failure::Inactive)
    );
    assert_eq!(calls, 3);
    assert_eq!(
        transaction.stage_replacement("retry.tmp", b"retry", || true),
        Err(Failure::Inactive)
    );
    assert_eq!(
        transaction.commit(|| panic!("failed stage must not reach final authority")),
        Err(Failure::Inactive)
    );
    assert_eq!(fs::read(&target).unwrap(), b"original synthetic bytes");
    assert!(!directory.join("owned-new.tmp").exists());
    assert!(!directory.join("retry.tmp").exists());
    assert_eq!(observed.load(Ordering::SeqCst), 1);
}

#[test]
#[ignore = "requires an explicit disposable Windows fixture root"]
fn production_staging_never_adopts_existing_temporary_or_replaces_first_failure() {
    let (directory, target) = fixture();
    let selection = selected(&target);
    let temporary = directory.join("previous.tmp");
    fs::write(&temporary, b"previous independent owner").unwrap();
    let (charge, observed) = charge();
    let mut transaction = Transaction::begin(Duration::from_secs(5), 2, charge).unwrap();
    transaction.retain_writer(&target, &selection).unwrap();
    drop(selection);
    let failure = transaction
        .stage_replacement("previous.tmp", b"replacement", || true)
        .unwrap_err();
    assert!(matches!(failure, Failure::Os(80 | 183)));
    assert_eq!(
        transaction.stage_replacement("different.tmp", b"replacement", || true),
        Err(failure)
    );
    assert_eq!(
        transaction.commit(|| panic!("prior OS failure must remain first")),
        Err(failure)
    );
    assert_eq!(fs::read(&temporary).unwrap(), b"previous independent owner");
    assert_eq!(fs::read(&target).unwrap(), b"original synthetic bytes");
    assert!(!directory.join("different.tmp").exists());
    assert_eq!(observed.load(Ordering::SeqCst), 1);
}

#[test]
#[ignore = "requires an explicit disposable Windows fixture root"]
fn production_staging_checks_reserved_capacity_and_single_component_before_authority_or_io() {
    for (capacity, temporary, expected) in [
        (1, "owned-new.tmp", Failure::Limit),
        (2, "../escape.tmp", Failure::InvalidBounds),
        (2, "nested\\escape.tmp", Failure::InvalidBounds),
        (2, "original.bin", Failure::InvalidBounds),
        (2, "alternate:stream", Failure::InvalidBounds),
    ] {
        let (directory, target) = fixture();
        let selection = selected(&target);
        let (charge, observed) = charge();
        let mut transaction = Transaction::begin(Duration::from_secs(5), capacity, charge).unwrap();
        transaction.retain_writer(&target, &selection).unwrap();
        drop(selection);
        assert_eq!(
            transaction.stage_replacement(temporary, b"replacement", || panic!(
                "invalid bounds before authority"
            )),
            Err(expected)
        );
        assert_eq!(
            transaction.commit(|| panic!("invalid stage cannot commit")),
            Err(expected)
        );
        assert_eq!(fs::read(&target).unwrap(), b"original synthetic bytes");
        assert_eq!(fs::read_dir(&directory).unwrap().count(), 1);
        assert_eq!(observed.load(Ordering::SeqCst), 1);
    }
}

#[test]
#[ignore = "requires an explicit disposable Windows fixture root"]
fn production_staging_cannot_dispatch_a_second_payload_or_commit_after_replay_attempt() {
    let (directory, target) = fixture();
    let selection = selected(&target);
    let (charge, observed) = charge();
    let mut transaction = Transaction::begin(Duration::from_secs(5), 2, charge).unwrap();
    transaction.retain_writer(&target, &selection).unwrap();
    drop(selection);
    transaction
        .stage_replacement("first.tmp", b"first payload", || true)
        .unwrap();
    assert_eq!(
        transaction.stage_replacement("second.tmp", b"second payload", || panic!(
            "no second authorization"
        )),
        Err(Failure::Inactive)
    );
    assert_eq!(
        transaction.commit(|| panic!("no commit after replay attempt")),
        Err(Failure::Inactive)
    );
    assert_eq!(fs::read(&target).unwrap(), b"original synthetic bytes");
    assert!(!directory.join("first.tmp").exists());
    assert!(!directory.join("second.tmp").exists());
    assert_eq!(observed.load(Ordering::SeqCst), 1);
}

#[test]
#[ignore = "requires an explicit disposable Windows fixture root"]
fn retained_local_ancestry_blocks_parent_rename_through_native_publish_and_closes_after_commit() {
    let (directory, target) = fixture();
    let selection = selected(&target);
    let moved = directory.with_extension("moved");
    let (charge, observed) = charge();
    let mut transaction = Transaction::begin(Duration::from_secs(5), 128, charge).unwrap();
    transaction
        .retain_local_writer(&target, &selection)
        .unwrap();
    drop(selection);
    assert!(fs::rename(&directory, &moved).is_err());
    assert!(!writable(&target));
    transaction
        .stage_replacement("local-owned.tmp", b"anchored replacement", || true)
        .unwrap();
    assert!(fs::rename(&directory, &moved).is_err());
    assert_eq!(fs::read(&target).unwrap(), b"original synthetic bytes");
    transaction
        .commit(|| {
            assert!(fs::rename(&directory, &moved).is_err());
            true
        })
        .unwrap();
    assert_eq!(observed.load(Ordering::SeqCst), 1);
    fs::rename(&directory, &moved).unwrap();
    assert_eq!(
        fs::read(moved.join("original.bin")).unwrap(),
        b"anchored replacement"
    );
    assert!(!moved.join("local-owned.tmp").exists());
}

#[test]
#[ignore = "requires an explicit disposable Windows fixture root"]
fn retained_local_ancestry_rejects_reparse_parent_without_locking_its_target() {
    let (directory, target) = fixture();
    let selection = selected(&target);
    let link = directory.with_extension("link");
    std::os::windows::fs::symlink_dir(&directory, &link).unwrap();
    let (charge, observed) = charge();
    let mut transaction = Transaction::begin(Duration::from_secs(5), 128, charge).unwrap();
    assert!(matches!(
        transaction.retain_local_writer(&link.join("original.bin"), &selection),
        Err(Failure::InvalidBounds)
    ));
    assert!(writable(&target));
    assert_eq!(
        transaction.stage_replacement("bad.tmp", b"replacement", || panic!(
            "reparse selection cannot dispatch"
        )),
        Err(Failure::InvalidBounds)
    );
    transaction.rollback().unwrap();
    assert_eq!(observed.load(Ordering::SeqCst), 1);
    drop(selection);
    fs::remove_dir(&link).unwrap();
    assert_eq!(fs::read(target).unwrap(), b"original synthetic bytes");
}

#[test]
#[ignore = "requires an explicit disposable Windows fixture root"]
fn retained_local_ancestry_reserves_the_whole_chain_before_opening_any_directory() {
    let (directory, target) = fixture();
    let selection = selected(&target);
    let moved = directory.with_extension("moved");
    let (charge, observed) = charge();
    let mut transaction = Transaction::begin(Duration::from_secs(5), 2, charge).unwrap();
    assert!(matches!(
        transaction.retain_local_writer(&target, &selection),
        Err(Failure::Limit)
    ));
    drop(selection);
    fs::rename(&directory, &moved).unwrap();
    assert!(writable(&moved.join("original.bin")));
    assert_eq!(
        transaction.commit(|| panic!("capacity failure cannot commit")),
        Err(Failure::Limit)
    );
    assert_eq!(observed.load(Ordering::SeqCst), 1);
    assert_eq!(
        fs::read(moved.join("original.bin")).unwrap(),
        b"original synthetic bytes"
    );
}

#[test]
#[ignore = "requires an explicit disposable Windows fixture root"]
fn retained_local_ancestry_never_substitutes_a_same_content_file_for_original_identity() {
    let (directory, target) = fixture();
    let selection = selected(&target);
    let original = directory.join("retained-original.bin");
    fs::rename(&target, &original).unwrap();
    fs::write(&target, b"original synthetic bytes").unwrap();
    let (charge, observed) = charge();
    let mut transaction = Transaction::begin(Duration::from_secs(5), 128, charge).unwrap();
    assert!(matches!(
        transaction.retain_local_writer(&target, &selection),
        Err(Failure::SelectionMismatch)
    ));
    assert_eq!(
        transaction.commit(|| panic!("different native file identity cannot commit")),
        Err(Failure::SelectionMismatch)
    );
    assert_eq!(observed.load(Ordering::SeqCst), 1);
    assert_eq!(fs::read(&target).unwrap(), b"original synthetic bytes");
    assert_eq!(fs::read(original).unwrap(), b"original synthetic bytes");
}
