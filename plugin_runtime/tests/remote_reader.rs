#![cfg(all(feature = "packages", target_os = "windows"))]
use morrow_core::{
    dispatch::{Connection, HostRuntime},
    shared_object::Descriptor,
    shared_transfer::Offer,
    store::Store,
};
use morrow_plugin_runtime::{
    remote_reader::RemoteReader,
    shared_objects::{Lease, Limits, SharedObjects},
};
use std::{
    io::Read,
    process::Command,
    time::{Duration, Instant},
};
fn setup(
    bytes: &[u8],
) -> (
    tempfile::TempDir,
    HostRuntime,
    Connection,
    SharedObjects,
    Descriptor,
    Lease,
) {
    let root = tempfile::tempdir().unwrap();
    let mut host =
        HostRuntime::new(Store::open(&root.path().join("db"), Default::default()).unwrap())
            .unwrap();
    let consumer = host.connect().unwrap();
    let mut objects = SharedObjects::new(&host, Limits::default()).unwrap();
    let desc = objects
        .publish(&host, &consumer, "scope:a", bytes, 1)
        .unwrap();
    let lease = objects
        .grant(&host, &consumer, &desc, "scope:a", 10, 1)
        .unwrap();
    (root, host, consumer, objects, desc, lease)
}
fn real() -> Command {
    Command::new(env!("CARGO_BIN_EXE_morrow-shared-reader"))
}
fn helper(root: &std::path::Path, mode: &str) -> Command {
    let mut command = Command::new(std::env::current_exe().unwrap());
    command
        .args(["--exact", "controlled_reader_child", "--nocapture"])
        .env("MORROW_READER_TEST_MODE", mode)
        .env("MORROW_READER_TEST_READY", root.join("ready"));
    command
}
fn ready(reader: &mut RemoteReader, path: &std::path::Path) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while !path.exists() {
        assert!(
            reader.poll_exit().unwrap().is_none(),
            "controlled child exited before marker"
        );
        assert!(Instant::now() < deadline, "child marker timeout");
        std::thread::sleep(Duration::from_millis(5));
    }
}
fn exit(reader: &mut RemoteReader) -> std::process::ExitStatus {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if let Some(status) = reader.poll_exit().unwrap() {
            return status;
        }
        if Instant::now() >= deadline {
            let _ = reader.stop();
            panic!("reader did not exit within deadline");
        }
        std::thread::sleep(Duration::from_millis(5));
    }
}
fn receive(
    reader: &mut RemoteReader,
    objects: &mut SharedObjects,
    host: &HostRuntime,
    consumer: &Connection,
) -> Vec<u8> {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if let Some(bytes) = reader.receive(objects, host, consumer, || 1).unwrap() {
            return bytes;
        }
        assert!(Instant::now() < deadline, "reply timeout");
        std::thread::sleep(Duration::from_millis(5));
    }
}
#[test]
fn controlled_reader_child() {
    let Ok(mode) = std::env::var("MORROW_READER_TEST_MODE") else {
        return;
    };
    let mut input = std::io::stdin().lock();
    let mut header = [0; 4];
    input.read_exact(&mut header).unwrap();
    let size = u32::from_le_bytes(header) as usize;
    assert!(size <= 128 * 1024);
    let mut bytes = vec![0; size];
    input.read_exact(&mut bytes).unwrap();
    let _offer = Offer::decode(&bytes).unwrap();
    std::fs::write(std::env::var("MORROW_READER_TEST_READY").unwrap(), b"alive").unwrap();
    if mode == "crash" {
        std::process::exit(91);
    }
    // Test harness stdout itself is intentionally an invalid protocol frame. Stay alive so
    // protocol failure can never be mistaken for process release. Do not write again:
    // the receiver may already have closed stdout after rejecting the harness frame.
    loop {
        std::thread::sleep(Duration::from_secs(1));
    }
}
#[test]
fn real_reply_does_not_release_pin_and_frozen_source_survives_original_mutation() {
    let mut original = b"immutable \0 source".to_vec();
    let (_root, host, c, mut b, desc, lease) = setup(&original);
    original.fill(99);
    let mut reader = b
        .start_reader(&host, &c, &lease, &mut real(), || 1)
        .unwrap();
    assert_eq!(
        receive(&mut reader, &mut b, &host, &c),
        b"immutable \0 source"
    );
    assert!(reader.poll_exit().unwrap().is_none());
    assert!(reader.receive(&mut b, &host, &c, || 1).is_err());
    b.retire(&desc).unwrap();
    assert_eq!(b.usage().objects, 1);
    assert_eq!(b.usage().charged_bytes, 65536);
    assert_eq!(b.usage().mappings, 1);
    reader.close_input();
    assert!(exit(&mut reader).success());
    assert_eq!(b.usage().charged_bytes, 0);
    assert_eq!(b.usage().mappings, 0);
    assert!(reader.poll_exit().unwrap().unwrap().success());
}
#[test]
fn revoked_expired_and_retired_leases_cannot_start_or_deliver_new_access() {
    for mode in 0..3 {
        let (_root, host, c, mut b, desc, lease) = setup(b"fixed");
        let mut reader = b
            .start_reader(&host, &c, &lease, &mut real(), || 1)
            .unwrap();
        match mode {
            0 => b.revoke(&lease).unwrap(),
            1 => {
                b.collect(10).unwrap();
            }
            _ => b.retire(&desc).unwrap(),
        };
        let tick = if mode == 1 { 10 } else { 1 };
        assert!(
            b.start_reader(&host, &c, &lease, &mut real(), || tick)
                .is_err()
        );
        assert!(reader.receive(&mut b, &host, &c, || tick).is_err());
        assert_eq!(b.usage().mappings, 1);
        if mode != 2 {
            b.retire(&desc).unwrap();
        }
        let _ = reader.stop();
        exit(&mut reader);
        assert_eq!(b.usage().charged_bytes, 0);
    }
}
#[test]
fn wrong_host_or_consumer_cannot_spawn_or_receive_anothers_lease() {
    let (_root, mut host, c, mut b, desc, lease) = setup(b"fixed");
    let other = host.connect().unwrap();
    let mut reader = b
        .start_reader(&host, &c, &lease, &mut real(), || 1)
        .unwrap();
    assert!(
        b.start_reader(&host, &other, &lease, &mut real(), || 1)
            .is_err()
    );
    assert!(reader.receive(&mut b, &host, &other, || 1).is_err());
    let (_d, h2, c2, mut b2, _, _) = setup(b"foreign");
    assert!(b.start_reader(&h2, &c2, &lease, &mut real(), || 1).is_err());
    assert!(reader.receive(&mut b2, &h2, &c2, || 1).is_err());
    assert_eq!(receive(&mut reader, &mut b, &host, &c), b"fixed");
    b.retire(&desc).unwrap();
    reader.close_input();
    exit(&mut reader);
    assert_eq!(b.usage().objects, 0);
}
#[test]
fn malformed_reply_while_child_is_live_retains_pin_until_actual_stop() {
    let (root, host, c, mut b, desc, lease) = setup(b"fixed");
    let mut reader = b
        .start_reader(&host, &c, &lease, &mut helper(root.path(), "bad"), || 1)
        .unwrap();
    ready(&mut reader, &root.path().join("ready"));
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        match reader.receive(&mut b, &host, &c, || 1) {
            Err(_) => break,
            Ok(None) => {
                assert!(Instant::now() < deadline);
                std::thread::sleep(Duration::from_millis(5));
            }
            Ok(Some(_)) => panic!("accepted malformed reply"),
        }
    }
    assert!(reader.poll_exit().unwrap().is_none());
    b.retire(&desc).unwrap();
    assert_eq!(b.usage().charged_bytes, 65536);
    reader.stop().unwrap();
    assert!(!exit(&mut reader).success());
    assert_eq!(b.usage().charged_bytes, 0);
}
#[test]
fn abnormal_exit_is_not_inferred_from_reply_error_but_reaped_by_actual_child() {
    let (root, host, c, mut b, desc, lease) = setup(b"fixed");
    let mut reader = b
        .start_reader(&host, &c, &lease, &mut helper(root.path(), "crash"), || 1)
        .unwrap();
    b.retire(&desc).unwrap();
    assert_eq!(b.usage().charged_bytes, 65536);
    let status = exit(&mut reader);
    assert_eq!(status.code(), Some(91));
    assert_eq!(b.usage().charged_bytes, 0);
    assert!(reader.receive(&mut b, &host, &c, || 1).is_err());
}
#[test]
fn dropping_live_reader_reaps_process_before_releasing_retired_allocation() {
    let (root, host, c, mut b, desc, lease) = setup(b"fixed");
    let mut reader = b
        .start_reader(&host, &c, &lease, &mut helper(root.path(), "hang"), || 1)
        .unwrap();
    ready(&mut reader, &root.path().join("ready"));
    assert!(reader.poll_exit().unwrap().is_none());
    b.retire(&desc).unwrap();
    assert_eq!(b.usage().charged_bytes, 65536);
    drop(reader);
    let deadline = Instant::now() + Duration::from_secs(10);
    while b.usage().charged_bytes != 0 {
        assert!(Instant::now() < deadline, "reaper failed to finish");
        std::thread::sleep(Duration::from_millis(5));
    }
    assert_eq!(b.usage().mappings, 0);
}
fn no_reply_program() -> Command {
    let mut c = Command::new(r"C:\Windows\System32\WindowsPowerShell\v1.0\powershell.exe");
    c.args(["-NoProfile","-NonInteractive","-Command","$s=[Console]::OpenStandardInput(); $b=New-Object byte[] 4096; while($s.Read($b,0,$b.Length) -gt 0) {}; exit 0"]);
    c
}
#[test]
fn no_reply_poll_is_nonblocking_and_explicit_stop_releases_only_after_exit() {
    let (_root, host, c, mut b, desc, lease) = setup(b"fixed");
    let mut reader = b
        .start_reader(&host, &c, &lease, &mut no_reply_program(), || 1)
        .unwrap();
    let before = Instant::now();
    assert!(reader.receive(&mut b, &host, &c, || 1).unwrap().is_none());
    assert!(before.elapsed() < Duration::from_secs(1));
    b.retire(&desc).unwrap();
    assert_eq!(b.usage().charged_bytes, 65536);
    reader.stop().unwrap();
    exit(&mut reader);
    assert_eq!(b.usage().charged_bytes, 0);
}
#[test]
fn closing_input_before_any_reply_really_delivers_eof() {
    let (_root, host, c, mut b, desc, lease) = setup(b"fixed");
    let mut reader = b
        .start_reader(&host, &c, &lease, &mut no_reply_program(), || 1)
        .unwrap();
    b.retire(&desc).unwrap();
    reader.close_input();
    assert!(exit(&mut reader).success());
    assert_eq!(b.usage().charged_bytes, 0);
}
#[test]
fn spawn_failure_never_leaves_an_unowned_mapping_pin() {
    let (root, host, c, mut b, desc, lease) = setup(b"fixed");
    assert!(
        b.start_reader(
            &host,
            &c,
            &lease,
            &mut Command::new(root.path().join("missing-reader.exe")),
            || 1
        )
        .is_err()
    );
    assert_eq!(b.usage().mappings, 0);
    b.retire(&desc).unwrap();
    assert_eq!(b.usage().objects, 0);
}
