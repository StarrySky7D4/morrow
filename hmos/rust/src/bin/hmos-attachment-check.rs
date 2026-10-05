//! Real linked Engine attachment self-check. Only a NEW isolated fixture is used.
use morrow_hmos::{Engine, Request};
use serde_json::json;
#[path = "../attachment_integration_tests.rs"]
mod scenarios;

#[cfg(unix)]
fn fd_ownership(root: &std::path::Path) -> serde_json::Value {
    use std::{
        ffi::{CStr, CString},
        os::fd::IntoRawFd,
    };
    unsafe extern "C" {
        fn fcntl(fd: i32, command: i32, ...) -> i32;
    }
    fn is_closed(fd: i32) -> bool {
        unsafe { fcntl(fd, 1) == -1 }
    }
    fn reply(ptr: *mut std::ffi::c_char) -> serde_json::Value {
        assert!(!ptr.is_null());
        let value = unsafe { serde_json::from_slice(CStr::from_ptr(ptr).to_bytes()).unwrap() };
        unsafe { morrow_hmos::morrow_hmos_free(ptr) };
        value
    }
    let source = root.join("fd-source");
    let output = root.join("fd-spool");
    std::fs::write(&source, b"FD ownership test").unwrap();
    let src = std::fs::File::open(&source).unwrap().into_raw_fd();
    let dst = std::fs::File::create(&output).unwrap().into_raw_fd();
    let result = reply(unsafe { morrow_hmos::morrow_hmos_prepare(src, dst, 64) });
    assert_eq!(result["ok"], true);
    assert!(is_closed(src));
    assert!(is_closed(dst));
    let output = std::fs::File::create(root.join("fd-invalid-prepare"))
        .unwrap()
        .into_raw_fd();
    let rejected = reply(unsafe { morrow_hmos::morrow_hmos_prepare(-1, output, 64) });
    assert_eq!(rejected["ok"], false);
    assert!(is_closed(output));
    let src = std::fs::File::open(&source).unwrap().into_raw_fd();
    let dst = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(&source)
        .unwrap()
        .into_raw_fd();
    let rejected = reply(unsafe { morrow_hmos::morrow_hmos_prepare(src, dst, 64) });
    assert_eq!(rejected["ok"], false);
    assert!(is_closed(src));
    assert!(is_closed(dst));
    assert_eq!(std::fs::read(&source).unwrap(), b"FD ownership test");
    let before = std::fs::read_dir("/proc/self/fd")
        .ok()
        .map(|entries| entries.count());
    for _ in 0..64 {
        let input = std::fs::File::open(&source).unwrap().into_raw_fd();
        let malformed = CString::new(r#"{"unknown_field":"no schema"}"#).unwrap();
        let result = reply(unsafe { morrow_hmos::morrow_hmos_import(malformed.as_ptr(), input) });
        assert_eq!(result["ok"], false);
        assert_eq!(result["error"], "InvalidRequest");
        assert!(is_closed(input));
    }
    let after = std::fs::read_dir("/proc/self/fd")
        .ok()
        .map(|entries| entries.count());
    if let (Some(before), Some(after)) = (before, after) {
        assert_eq!(before, after);
    }
    // Invalid output schema also consumes the transferred output descriptor.
    let output = std::fs::File::create(root.join("fd-error-output"))
        .unwrap()
        .into_raw_fd();
    let malformed = CString::new("not JSON").unwrap();
    let result = reply(unsafe { morrow_hmos::morrow_hmos_export(malformed.as_ptr(), output) });
    assert_eq!(result["ok"], false);
    assert!(is_closed(output));
    json!({"prepare_owned_descriptors":"PASS","invalid_prepare_consumes_output":"PASS","same_inode_prepare_rejects_and_closes_both":"PASS",
        "import_invalid_schema_closes_64_descriptors":"PASS","export_invalid_schema_closes_descriptor":"PASS","fd_count_before":before,"fd_count_after":after})
}
fn main() {
    let base = std::env::args()
        .nth(1)
        .expect("provide NEW isolated fixture directory");
    let root = std::path::Path::new(&base);
    std::fs::create_dir(root).expect("fixture directory must not already exist");
    let functional = scenarios::run_round_trip(&root.join("functional"));
    println!("functional: {functional}");
    let failures = scenarios::run_failed_streams(&root.join("failures"));
    println!("failures: {failures}");
    let capacity = scenarios::run_capacity(&root.join("capacity"));
    println!("capacity: {capacity}");
    #[cfg(unix)]
    let descriptors = fd_ownership(root);
    #[cfg(not(unix))]
    let descriptors = json!({"native_descriptor_ownership":"NOT_RUN non-Unix host"});
    println!(
        "{}",
        json!({"status":"PASS_SCOPED","profile":"development-unsealed","isolated_fixture":base,
        "functional":functional,"failures":failures,"capacity":capacity,"descriptors":descriptors,
        "production_capture_huks_device_picker_ui":"NOT_QUALIFIED"})
    );
}
