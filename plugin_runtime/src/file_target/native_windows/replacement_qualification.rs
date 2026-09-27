//! Explicit Windows qualification, not a production replacement entry point.
use super::*;
use std::{
    fs,
    fs::OpenOptions,
    io::{Read, Seek, SeekFrom, Write},
    os::windows::fs::OpenOptionsExt,
};

#[test]
#[ignore = "explicit Windows capability experiment; reports OS rejection without relaxing original locks"]
fn posix_replace_while_original_and_parent_remain_pinned() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("original.bin");
    let alias = dir.path().join("original-alias.bin");
    fs::write(&path, b"old-object").unwrap();
    fs::hard_link(&path, &alias).unwrap();
    let parent = OpenOptions::new()
        .access_mode(0x0010_00a0)
        .share_mode(1)
        .custom_flags(0x0220_0000)
        .open(dir.path())
        .unwrap();
    let mut original = OpenOptions::new()
        .access_mode(0xc011_0000)
        .share_mode(0)
        .custom_flags(0x0020_0000)
        .open(&path)
        .unwrap();
    let mut replacement = open_new(&parent, "candidate.tmp").unwrap();
    replacement.write_all(b"new-object").unwrap();
    replacement.sync_all().unwrap();
    let result = posix_publish(&replacement, "original.bin");
    let code = if result < 0 {
        unsafe { RtlNtStatusToDosError(result) }
    } else {
        0
    };
    println!(
        "REPLACE_QUALIFICATION ntstatus={:#010x} win32={} original_share=0 parent_share=READ",
        result as u32, code
    );
    original.seek(SeekFrom::Start(0)).unwrap();
    let mut retained = Vec::new();
    original.read_to_end(&mut retained).unwrap();
    assert_eq!(retained, b"old-object");
    drop(replacement);
    drop(original);
    drop(parent);
    assert_eq!(fs::read(&alias).unwrap(), b"old-object");
    if result >= 0 {
        assert_eq!(fs::read(&path).unwrap(), b"new-object");
        assert!(!dir.path().join("candidate.tmp").exists());
        println!("RENAME_SYSCALL_SUCCEEDED_IN_THIS_FIXTURE; NOT_CONDITIONAL_REPLACEMENT_PROOF");
    } else {
        assert_eq!(fs::read(&path).unwrap(), b"old-object");
        assert_eq!(
            fs::read(dir.path().join("candidate.tmp")).unwrap(),
            b"new-object"
        );
        println!(
            "REPLACE_CAPABILITY_REJECTED_IN_THIS_FIXTURE: production effect must remain disabled"
        );
    }
}

fn posix_publish(source: &File, leaf: &str) -> i32 {
    // repr(C) layout/flags follow FILE_RENAME_INFORMATION; class65 is the Ex
    // operation. Only these disposable test files are reachable by the handle.
    let mut info: RenameInformation = unsafe { std::mem::zeroed() };
    info.flags = 3; // REPLACE_IF_EXISTS | POSIX_SEMANTICS; no ignore-readonly flag
    let name: Vec<_> = leaf.encode_utf16().collect();
    info.file_name[..name.len()].copy_from_slice(&name);
    info.file_name_length = (name.len() * 2) as u32;
    let mut status = IoStatusBlock {
        status: IoStatus {
            pointer: std::ptr::null_mut(),
        },
        information: 0,
    };
    // SAFETY: synchronous owned handle; valid aligned input and output for this
    // call; NULL root and single name mean source's own directory, no CWD path.
    let result = unsafe {
        NtSetInformationFile(
            source.as_raw_handle(),
            &mut status,
            (&info as *const RenameInformation).cast(),
            size_of::<RenameInformation>() as u32,
            65,
        )
    };
    result
}

#[test]
#[ignore = "explicit counterexample; delete-sharing experiment never changes production selection policy"]
fn allowing_delete_sharing_does_not_make_replacement_conditional() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("original.bin");
    fs::write(&path, b"selected-old-object").unwrap();
    let parent = OpenOptions::new()
        .access_mode(0x0010_00a0)
        .share_mode(1)
        .custom_flags(0x0220_0000)
        .open(dir.path())
        .unwrap();
    // EXPERIMENT ONLY: this is intentionally weaker than production share=0.
    // It demonstrates why enabling deletion sharing is not an acceptable fix.
    let mut selected = OpenOptions::new()
        .access_mode(0xc011_0000)
        .share_mode(4)
        .custom_flags(0x0020_0000)
        .open(&path)
        .unwrap();
    let stamp_before = selected.metadata().unwrap();
    let mut other = open_new(&parent, "other.tmp").unwrap();
    other.write_all(b"concurrent-new-object").unwrap();
    other.sync_all().unwrap();
    let first = posix_publish(&other, "original.bin");
    if first < 0 {
        let code = unsafe { RtlNtStatusToDosError(first) };
        println!(
            "CAS_COUNTEREXAMPLE_UNAVAILABLE ntstatus={:#010x} win32={}",
            first as u32, code
        );
        drop(other);
        drop(selected);
        drop(parent);
        assert_eq!(fs::read(&path).unwrap(), b"selected-old-object");
        return;
    }
    drop(other);
    assert_eq!(fs::read(&path).unwrap(), b"concurrent-new-object");
    // The retained object and the metadata used by the old precondition still
    // describe the original, while its previous name now denotes a different file.
    assert_eq!(selected.metadata().unwrap().len(), stamp_before.len());
    selected.seek(SeekFrom::Start(0)).unwrap();
    let mut old = Vec::new();
    selected.read_to_end(&mut old).unwrap();
    assert_eq!(old, b"selected-old-object");
    let mut ours = open_new(&parent, "ours.tmp").unwrap();
    ours.write_all(b"our-new-object").unwrap();
    ours.sync_all().unwrap();
    let second = posix_publish(&ours, "original.bin");
    assert!(
        second >= 0,
        "second replacement ntstatus={:#010x}",
        second as u32
    );
    drop(ours);
    drop(selected);
    drop(parent);
    assert_eq!(fs::read(&path).unwrap(), b"our-new-object");
    println!(
        "CAS_COUNTEREXAMPLE_CONFIRMED: retained old handle survived but concurrent target was overwritten"
    );
}
