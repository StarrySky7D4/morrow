//! Actual Windows close refusal of this test's newly owned directory only.
use super::*;

#[test]
fn actual_protected_directory_close_preserves_original_native_error_without_retry() {
    let temp = tempfile::tempdir().unwrap();
    assert!(temp.path().starts_with(std::path::Path::new("D:/build_cache")));
    let directory = temp.path().join("actual-close-refusal");
    std::fs::create_dir(&directory).unwrap();
    let file = OpenOptions::new().read(true).share_mode(3).custom_flags(0x0220_0000).open(&directory).unwrap();
    let raw = file.as_raw_handle();
    protect_close_fixture(&file).unwrap();
    let mut original = NativeFile::new(file);
    original.close_once(false);
    assert!(original.attempted());
    assert!(original.unknown());
    assert!(!original.closed());
    assert_eq!(original.error, Some(6));
    assert_eq!(original.raw, raw as usize);
    assert!(original.file().is_none());
    // The original method refuses re-entry before another OS call. No flag is
    // removed and no alternate handle is manufactured to clean up the failure.
    original.close_once(false);
    assert_eq!(original.error, Some(6));
    assert!(original.unknown() && !original.closed());
    assert_eq!(original.raw, raw as usize);
    let _retained_fixture = temp.keep();
    std::mem::forget(original);
    // Test-process exit is never represented as original native retirement.
}
