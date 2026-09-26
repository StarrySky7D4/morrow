use super::*;
use morrow_plugin_runtime::file_io::CaptureError;
use std::io::{Seek, SeekFrom, Write};

#[test]
fn open_file_capture_survives_source_replacement_and_guest_reads_exact_chunks() {
    let mut f = Fixture::new(true, 2, 2_000_000);
    let instance = f.connect();
    let binding = f.bind(&instance, 90, 1);
    let mut broker = FileBroker::new([0x91; 32]);
    let original = (0..90_007).map(|i| (i % 251) as u8).collect::<Vec<_>>();
    let path = f._dir.path().join("selected.bin");
    std::fs::write(&path, &original).unwrap();
    let mut file = std::fs::File::open(&path).unwrap();
    file.seek(SeekFrom::Start(19)).unwrap();
    let moved = f._dir.path().join("original-moved.bin");
    std::fs::rename(&path, &moved).unwrap();
    std::fs::write(&path, b"path now points to a different file").unwrap();
    let selected = broker
        .grant_open_file(
            &f.manager,
            &f.host,
            &instance,
            &binding,
            file,
            original.len() as u64,
            || 2,
        )
        .unwrap();
    assert_eq!(selected.length, original.len() as u64);
    assert_eq!(
        selected.sha256,
        morrow_core::runtime::schema_digest(&original)
    );
    assert_eq!(binding.usage().bytes, selected.length + 1);
    assert_eq!(binding.usage().jobs, 0);
    assert_eq!(binding.usage().resources, 1);
    std::fs::write(&path, b"replacement").unwrap();
    std::fs::remove_file(&path).unwrap();
    std::fs::remove_file(&moved).unwrap();
    let mut all = Vec::new();
    for (id, offset) in [(1, 0), (2, 65_536)] {
        let request = Request::encode_read(id, &selected.reference, offset, 0).unwrap();
        let report = broker.run(
            &f.manager,
            &f.host,
            &instance,
            &binding,
            HANDLER,
            &request,
            || 3,
        );
        assert_eq!(report.execution.outcome, Ok(0));
        let response = report.response.unwrap();
        assert_eq!(response.status, Status::Completed);
        assert_eq!(response.eof, offset > 0);
        all.extend(response.payload);
    }
    assert_eq!(all, original);
    let finish = Request::encode_finish(3, &selected.reference).unwrap();
    assert_eq!(
        exchange(&f, &mut broker, &instance, &binding, &finish, 4).status,
        Status::Completed
    );
    assert_eq!(broker.usage(), (0, 0));
    assert_eq!(binding.usage().resources, 0);
    instance.close(&mut f.host).unwrap();
}

#[test]
fn capture_limits_and_foreign_identity_fail_before_seek_read_or_charge() {
    for case in ["host-limit", "job-limit", "resource-limit", "foreign"] {
        let mut f = Fixture::new(true, 1, if case == "job-limit" { 10 } else { 1000 });
        let instance = f.connect();
        let binding = f.bind(&instance, 90, 1);
        let other = f.connect();
        let mut broker = FileBroker::new([0x92; 32]);
        if case == "resource-limit" {
            broker
                .grant_file(&f.manager, &f.host, &instance, &binding, vec![], 2)
                .unwrap();
        }
        let before = binding.usage();
        let usage = broker.usage();
        let mut file = tempfile::tempfile().unwrap();
        file.write_all(b"0123456789").unwrap();
        file.seek(SeekFrom::Start(3)).unwrap();
        let mut cursor = file.try_clone().unwrap();
        let error = broker
            .grant_open_file(
                &f.manager,
                &f.host,
                if case == "foreign" { &other } else { &instance },
                &binding,
                file,
                if case == "host-limit" { 9 } else { 10 },
                || 3,
            )
            .unwrap_err();
        assert_eq!(
            error,
            CaptureError::Admission(if case == "foreign" {
                morrow_plugin_runtime::io_binding::Error::Denied
            } else {
                morrow_plugin_runtime::io_binding::Error::Limit
            }),
            "{case}"
        );
        assert_eq!(cursor.stream_position().unwrap(), 3, "{case}: no seek/read");
        assert_eq!(binding.usage(), before);
        assert_eq!(broker.usage(), usage);
        other.close(&mut f.host).unwrap();
        instance.close(&mut f.host).unwrap();
    }
}

#[test]
fn capture_stop_expiry_and_clock_rollback_never_publish_or_refund() {
    for failure in ["stop", "expiry", "clock"] {
        // Single-chunk capture: before allocation, after data read, after digest.
        for fail_at in [3, 6, 9] {
            let mut f = Fixture::new(true, 1, 1000);
            let instance = f.connect();
            let binding = f.bind(&instance, 90, 1);
            let mut broker = FileBroker::new([0x93; 32]);
            let mut file = tempfile::tempfile().unwrap();
            file.write_all(b"0123456789").unwrap();
            let mut ticks = 0;
            let result =
                broker.grant_open_file(&f.manager, &f.host, &instance, &binding, file, 10, || {
                    ticks += 1;
                    if ticks >= 3 {
                        assert_eq!(binding.usage().jobs, 1);
                        assert_eq!(binding.usage().resources, 1);
                        assert_eq!(binding.usage().bytes, 11);
                    }
                    if ticks == fail_at {
                        match failure {
                            "stop" => instance.stop(),
                            "expiry" => return 90,
                            "clock" => return 1,
                            _ => unreachable!(),
                        }
                    }
                    2
                });
            assert_eq!(ticks, fail_at);
            let expected = match failure {
                "stop" => morrow_plugin_runtime::io_binding::Error::Denied,
                "expiry" => morrow_plugin_runtime::io_binding::Error::Expired,
                _ => morrow_plugin_runtime::io_binding::Error::Clock,
            };
            assert_eq!(
                result,
                Err(CaptureError::Admission(expected)),
                "{failure} at {fail_at}"
            );
            assert_eq!(broker.usage(), (0, 0));
            assert_eq!(binding.usage().resources, 0);
            assert_eq!(binding.usage().jobs, 0);
            assert_eq!(binding.usage().bytes, 11);
            instance.close(&mut f.host).unwrap();
        }
    }
}

#[test]
fn source_growth_shrink_and_read_failure_release_reserved_slots() {
    for change in ["grow", "shrink", "read-failure"] {
        let mut f = Fixture::new(true, 1, 1000);
        let instance = f.connect();
        let binding = f.bind(&instance, 90, 1);
        let mut broker = FileBroker::new([0x94; 32]);
        let path = f._dir.path().join("mutable.bin");
        std::fs::write(&path, b"0123456789").unwrap();
        let file = std::fs::OpenOptions::new()
            .read(change != "read-failure")
            .write(true)
            .open(&path)
            .unwrap();
        let editor = std::fs::OpenOptions::new().write(true).open(&path).unwrap();
        let mut ticks = 0;
        let result =
            broker.grant_open_file(&f.manager, &f.host, &instance, &binding, file, 10, || {
                ticks += 1;
                if ticks == 3 && change != "read-failure" {
                    editor
                        .set_len(if change == "grow" { 11 } else { 9 })
                        .unwrap();
                }
                2
            });
        if change == "read-failure" {
            assert!(matches!(result, Err(CaptureError::Io(_))));
        } else {
            assert_eq!(result, Err(CaptureError::SourceChanged));
        }
        assert_eq!(broker.usage(), (0, 0));
        assert_eq!(binding.usage().resources, 0);
        assert_eq!(binding.usage().jobs, 0);
        assert_eq!(binding.usage().bytes, 11);
        instance.close(&mut f.host).unwrap();
    }
}

#[test]
fn empty_file_requires_probe_budget_and_has_empty_digest() {
    let mut f = Fixture::new(true, 1, 1000);
    let instance = f.connect();
    let binding = f.bind(&instance, 90, 1);
    let mut broker = FileBroker::new([0x95; 32]);
    let selected = broker
        .grant_open_file(
            &f.manager,
            &f.host,
            &instance,
            &binding,
            tempfile::tempfile().unwrap(),
            0,
            || 2,
        )
        .unwrap();
    assert_eq!(selected.length, 0);
    assert_eq!(selected.sha256, morrow_core::runtime::schema_digest(&[]));
    assert_eq!(binding.usage().bytes, 1);
    let request = Request::encode_read(1, &selected.reference, 0, 0).unwrap();
    let response = exchange(&f, &mut broker, &instance, &binding, &request, 3);
    assert!(response.payload.is_empty() && response.eof);
    let cancel = Request::encode_cancel(2, &selected.reference).unwrap();
    assert_eq!(
        exchange(&f, &mut broker, &instance, &binding, &cancel, 4).status,
        Status::Completed
    );
    assert_eq!(broker.usage(), (0, 0));
    instance.close(&mut f.host).unwrap();
}

#[cfg(unix)]
#[test]
fn directory_and_device_are_rejected_without_reservation() {
    let mut f = Fixture::new(true, 1, 1000);
    let instance = f.connect();
    let binding = f.bind(&instance, 90, 1);
    let mut broker = FileBroker::new([0x96; 32]);
    for path in [f._dir.path(), std::path::Path::new("/dev/zero")] {
        let file = std::fs::File::open(path).unwrap();
        let before = binding.usage();
        assert_eq!(
            broker.grant_open_file(&f.manager, &f.host, &instance, &binding, file, 100, || 2),
            Err(CaptureError::NotRegularFile)
        );
        assert_eq!(binding.usage(), before);
        assert_eq!(broker.usage(), (0, 0));
    }
    instance.close(&mut f.host).unwrap();
}

#[test]
fn digest_describes_retained_bytes_even_when_source_changes_at_same_length() {
    let mut f = Fixture::new(true, 1, 2_000_000);
    let instance = f.connect();
    let binding = f.bind(&instance, 90, 1);
    let mut broker = FileBroker::new([0x97; 32]);
    let path = f._dir.path().join("mixed-time.bin");
    std::fs::write(&path, vec![1; 90_007]).unwrap();
    let file = std::fs::File::open(&path).unwrap();
    let mut ticks = 0;
    let selected = broker
        .grant_open_file(
            &f.manager,
            &f.host,
            &instance,
            &binding,
            file,
            90_007,
            || {
                ticks += 1;
                if ticks == 6 {
                    std::fs::write(&path, vec![2; 90_007]).unwrap();
                }
                2
            },
        )
        .unwrap();
    let mut retained = vec![1; 65_536];
    retained.extend(vec![2; 90_007 - 65_536]);
    assert_eq!(
        selected.sha256,
        morrow_core::runtime::schema_digest(&retained)
    );
    let request = Request::encode_read(1, &selected.reference, 65_536, 0).unwrap();
    let response = exchange(&f, &mut broker, &instance, &binding, &request, 3);
    assert_eq!(response.payload, &retained[65_536..]);
    instance.close(&mut f.host).unwrap();
    broker.reap(4);
    assert_eq!(broker.usage(), (0, 0));
}

#[test]
fn file_list_only_binding_cannot_capture_or_advance_shared_clock() {
    let declared = BTreeSet::from([IoCapability::FileRead, IoCapability::FileList]);
    let mut f = Fixture::with_capabilities(true, 2, 1000, declared);
    let instance = f.connect();
    let binding = f
        .manager
        .bind_io(
            &f.host,
            &instance,
            f.package.digest(),
            f.manager.revision(),
            &BTreeSet::from([IoCapability::FileList]),
            90,
            1,
        )
        .unwrap();
    let mut broker = FileBroker::new([0x98; 32]);
    let mut file = tempfile::tempfile().unwrap();
    file.write_all(b"0123456789").unwrap();
    file.seek(SeekFrom::Start(3)).unwrap();
    let mut observer = file.try_clone().unwrap();
    let before = binding.usage();
    assert_eq!(
        broker.grant_open_file(&f.manager, &f.host, &instance, &binding, file, 10, || 88,),
        Err(CaptureError::Admission(
            morrow_plugin_runtime::io_binding::Error::Denied
        ))
    );
    assert_eq!(observer.stream_position().unwrap(), 3);
    assert_eq!(binding.usage(), before);
    let approved = f.bind(&instance, 90, 2);
    let selected = broker
        .grant_open_file(
            &f.manager,
            &f.host,
            &instance,
            &approved,
            observer,
            10,
            || 3,
        )
        .unwrap();
    assert_eq!(selected.length, 10);
    instance.close(&mut f.host).unwrap();
    broker.reap(4);
    assert_eq!(broker.usage(), (0, 0));
}
