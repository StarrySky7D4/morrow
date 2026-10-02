//! Synthetic workspace-only fixture. Not a product entry point.
use std::{
    io::{self, Read, Write},
    time::Duration,
};
fn main() {
    let args: Vec<_> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("controller-harness-closed-stdio") => {
            controller_harness_closed_stdio(&args[1], &args[2], &args[3])
        }
        Some("bootstrap-ordinary-reject") => {
            use std::{fs, os::fd::AsRawFd};
            let ordinary = fs::File::open(&args[1]).unwrap();
            fs::remove_file(&args[1]).unwrap();
            assert_eq!(unsafe { libc::dup2(ordinary.as_raw_fd(), 6) }, 6);
            let result =
                unsafe { morrow_linux_supervisor_foundation::take_controller_fixture_transport() };
            let error = result.err().unwrap().to_string();
            // Ordinary tmpfs may return F_SEAL_SEAL; other filesystems return
            // -1/EINVAL. Both reject before interpreting any bootstrap bytes.
            assert!(
                error.contains("F_GET_SEALS") || error.contains("controller bootstrap seal policy")
            );
            println!("ordinary-unlinked-bootstrap-rejected-before-read");
        }
        Some("controller-peer") => controller_peer(&args[1]),
        Some("supervisor-peer") => supervisor_peer(
            &args[1],
            args.get(2).map(String::as_str).unwrap_or("normal"),
        ),

        Some("closed-stdio-check") => {
            use morrow_linux_supervisor_foundation::{OwnedProcess, SealedExecutable, digest};
            use std::{ffi::OsStr, fs, path::Path};
            let artifact = Path::new(&args[1]);
            let cwd = Path::new(&args[2]);
            let receipt = Path::new(&args[3]);
            let expected = digest(&fs::read(artifact).unwrap());
            // Separate subprocess: never close the test harness's stdio.
            unsafe {
                libc::close(0);
                libc::close(1);
                libc::close(2);
            }
            let executable = SealedExecutable::read(artifact, expected).unwrap();
            let mut child = OwnedProcess::spawn(executable, cwd, &[OsStr::new("echo")]).unwrap();
            let done = child.wait_bounded(Duration::from_secs(2)).unwrap();
            assert!(done && child.cleanup_complete());
            assert!(child.output().stdout.starts_with(b"sealed-main-elf\n"));
            assert_eq!(child.output().stderr, b"sealed-stderr\n");
            fs::write(receipt, b"closed-stdio-sealed-exec-pidfd-cleanup-ok\n").unwrap();
        }
        Some("fd-check") => {
            let extra = (3..1024)
                .filter(|fd| unsafe { libc::fcntl(*fd, libc::F_GETFD) } >= 0)
                .count();
            println!("extra-open-fds={extra}");
        }
        Some("echo") => {
            println!("sealed-main-elf");
            eprintln!("sealed-stderr");
            println!("env-count={}", std::env::vars_os().count());
            let mut input = String::new();
            io::stdin().read_to_string(&mut input).unwrap();
            println!("stdin-bytes={}", input.len());
            println!("cwd={}", std::env::current_dir().unwrap().display());
            std::process::exit(17);
        }
        Some("sleep") => std::thread::sleep(Duration::from_secs(30)),
        Some("flood") => {
            io::stdout().write_all(&vec![b'o'; 256 * 1024]).unwrap();
            io::stderr().write_all(&vec![b'e'; 128 * 1024]).unwrap();
        }
        Some("retain-output") => {
            // Deliberately demonstrate that a pidfd is not process-tree ownership.
            // Single-threaded fixture; child uses only libc sleep/_exit.
            let pid = unsafe { libc::fork() };
            assert!(pid >= 0);
            if pid == 0 {
                unsafe {
                    libc::usleep(200_000);
                    libc::_exit(0);
                }
            }
        }
        _ => std::process::exit(2),
    }
}

fn controller_peer(mode: &str) {
    use morrow_linux_supervisor_foundation::{
        FrameKind, FrameRead, take_controller_fixture_transport,
    };
    let mut transport = unsafe { take_controller_fixture_transport() }.unwrap();
    let started = std::time::Instant::now();
    let mut ready = false;
    let mut last_heartbeat = started.elapsed();
    let mut replied = false;
    let mut closing = false;
    while started.elapsed() < Duration::from_secs(3) {
        let now = started.elapsed();
        if transport.poll_write(now).is_err() {
            transport.retire();
            return;
        }
        if mode == "stall" && replied {
            std::thread::sleep(Duration::from_millis(2));
            continue;
        }
        match transport.poll_read(now) {
            Ok(FrameRead::Frame(frame)) if frame.kind() == FrameKind::Reply => {
                assert_eq!(frame.payload(), b"fixture-controller-data");
                replied = true;
                if mode == "retain-and-exit" {
                    let pid = unsafe { libc::fork() };
                    assert!(pid >= 0);
                    if pid == 0 {
                        unsafe {
                            libc::usleep(350_000);
                            libc::_exit(0);
                        }
                    }
                    std::process::exit(0);
                }
                if mode == "eof-alive" {
                    transport.retire();
                    std::thread::sleep(Duration::from_millis(500));
                    return;
                }
                if mode == "normal" && !closing {
                    transport.queue(FrameKind::GracefulClose, &[], now).unwrap();
                    closing = true;
                }
            }
            Ok(FrameRead::Frame(frame))
                if frame.kind() == FrameKind::Status
                    && frame.payload() == b"preparing-synthetic-artifact" =>
            {
                if mode == "exit-in-preparation" {
                    std::process::exit(0);
                }
            }
            Ok(FrameRead::Frame(frame))
                if frame.kind() == FrameKind::Status
                    && frame.payload() == b"preparing-actual-launch" =>
            {
                if mode == "exit-in-launch-preparation" {
                    std::process::exit(0);
                }
            }
            Ok(FrameRead::Frame(frame))
                if frame.kind() == FrameKind::Status
                    && frame.payload() == b"bootstrap-ready"
                    && !ready =>
            {
                ready = true;
                transport.queue(FrameKind::Heartbeat, &[], now).unwrap();
                transport
                    .queue(FrameKind::Data, b"fixture-controller-data", now)
                    .unwrap();
            }
            Ok(FrameRead::Frame(frame)) if frame.kind() == FrameKind::Status => {
                assert_eq!(frame.payload(), b"tracked-direct-cleanup-only");
                transport.retire();
                println!("controller-tracked-status;product-owner=false");
                return;
            }
            Ok(FrameRead::Eof) | Err(_) => {
                transport.retire();
                return;
            }
            _ => (),
        }
        if ready
            && now.saturating_sub(last_heartbeat) >= Duration::from_millis(100)
            && !(mode == "stall" && replied)
        {
            if transport.queue(FrameKind::Heartbeat, &[], now).is_err() {
                transport.retire();
                return;
            }
            last_heartbeat = now;
        }
        std::thread::sleep(Duration::from_millis(2));
    }
    transport.retire();
}

fn supervisor_peer(artifact: &str, mode: &str) {
    use morrow_linux_supervisor_foundation::{
        ControllerLoss, FrameKind, FrameRead, OwnedProcess, SealedExecutable,
        take_supervisor_fixture_transport,
    };
    use std::{ffi::OsStr, path::Path};
    let (mut transport, mut watch) = unsafe { take_supervisor_fixture_transport() }.unwrap();
    let started = std::time::Instant::now();
    transport
        .queue(
            FrameKind::Status,
            b"preparing-synthetic-artifact",
            started.elapsed(),
        )
        .unwrap();
    let _ = transport.poll_write(started.elapsed()).unwrap();
    if mode == "delayed-preparation" {
        std::thread::sleep(Duration::from_millis(100));
    }
    let executable =
        SealedExecutable::read(Path::new(artifact), transport.bound_artifact_digest()).unwrap();
    let cwd = std::env::current_dir().unwrap();
    let child_mode = if mode == "guest-fd-check" {
        "fd-check"
    } else {
        "sleep"
    };
    let mut prepared_child = Some(
        OwnedProcess::prepare_controller_fixture(executable, &cwd, &[OsStr::new(child_mode)])
            .unwrap(),
    );
    if !watch.poll() {
        transport.retire();
        println!(
            "gate-closed=true;loss={:?};no-dependent-child;data-admitted=0;tree-empty=unproved;product-owner=false",
            watch.loss().map(|v| v.reason)
        );
        return;
    }
    transport
        .queue(FrameKind::Status, b"bootstrap-ready", started.elapsed())
        .unwrap();
    let mut child = None::<OwnedProcess>;
    let mut closing = false;
    let mut stopped = false;
    let mut status_queued = false;
    let mut data_admitted = 0;
    while started.elapsed() < Duration::from_secs(3) {
        let now = started.elapsed();
        if !watch.poll() {
            closing = true;
        }
        if transport.healthy() {
            match transport.poll_read(now) {
                Ok(FrameRead::Frame(frame)) => match frame.kind() {
                    FrameKind::Heartbeat => {
                        let _ = watch.heartbeat(&frame);
                    }
                    FrameKind::Data
                        if !closing
                            && watch.poll()
                            && watch.admission_open()
                            && child.is_none() =>
                    {
                        if mode == "delayed-launch" {
                            transport
                                .queue(FrameKind::Status, b"preparing-actual-launch", now)
                                .unwrap();
                            let _ = transport.poll_write(now).unwrap();
                            std::thread::sleep(Duration::from_millis(100));
                        }
                        match prepared_child.take().unwrap().spawn(&mut watch) {
                            Ok(created) => child = Some(created),
                            Err(morrow_linux_supervisor_foundation::SpawnFailure::NotCreated(
                                _,
                            )) => {
                                transport.retire();
                                println!(
                                    "gate-closed=true;loss={:?};no-dependent-child;data-admitted=0;child-created=0;tree-empty=unproved;product-owner=false",
                                    watch.loss().map(|v| v.reason)
                                );
                                return;
                            }
                            Err(morrow_linux_supervisor_foundation::SpawnFailure::Created {
                                owner,
                                ..
                            }) => {
                                child = Some(*owner);
                                closing = true;
                            }
                        }
                        data_admitted += 1;
                        if !closing && watch.poll() {
                            transport
                                .queue(FrameKind::Reply, frame.payload(), started.elapsed())
                                .unwrap();
                        }
                    }
                    FrameKind::GracefulClose => closing = true,
                    _ => watch.revoke(ControllerLoss::Protocol),
                },
                Ok(FrameRead::Eof) => watch.revoke(ControllerLoss::TransportEof),
                Ok(FrameRead::Pending) => (),
                Err(_) => watch.revoke(ControllerLoss::Protocol),
            }
        }
        let now = started.elapsed(); // preparation/exec may have consumed time
        if watch.gate_closed() {
            closing = true;
        }
        if transport.healthy() && transport.poll_write(now).is_err() {
            watch.revoke(ControllerLoss::Backpressure);
            closing = true;
        }
        if let Some(child) = child.as_mut() {
            if closing && !stopped {
                child.terminate().unwrap();
                stopped = true;
            }
            child.observe().unwrap();
            if closing && child.cleanup_complete() {
                if !watch.gate_closed() && transport.healthy() && !status_queued {
                    transport
                        .queue(FrameKind::Status, b"tracked-direct-cleanup-only", now)
                        .unwrap();
                    status_queued = true;
                    continue;
                }
                if !transport.healthy() || watch.gate_closed() || transport.queued_bytes() == 0 {
                    transport.retire();
                    if mode == "guest-fd-check" {
                        assert_eq!(child.output().stdout, b"extra-open-fds=0\n");
                        println!("guest-extra-open-fds=0");
                    }
                    println!(
                        "gate-closed=true;loss={:?};data-admitted={};direct-child-reaped={};stdout-eof={};stderr-eof={};control-io-retired={};tree-empty=unproved;product-owner=false",
                        watch.loss().map(|v| v.reason),
                        data_admitted,
                        child.exit_observation().is_some(),
                        child.output().stdout_eof,
                        child.output().stderr_eof,
                        transport.io_retired()
                    );
                    return;
                }
            }
        } else if closing {
            transport.retire();
            println!("gate-closed=true;no-dependent-child;tree-empty=unproved;product-owner=false");
            return;
        }
        std::thread::sleep(Duration::from_millis(2));
    }
    transport.retire();
    if let Some(child) = child.as_mut() {
        child.terminate().unwrap();
        let _ = child.wait_bounded(Duration::from_secs(1));
    }
    println!("retained-unconfirmed;tree-empty=unproved;product-owner=false");
}

fn controller_harness_closed_stdio(artifact: &str, cwd: &str, receipt: &str) {
    use morrow_linux_supervisor_foundation::{
        ControllerLimits, SealedExecutable, digest, fixture_channel_pair,
    };
    use std::{ffi::OsStr, fs, path::Path};
    let artifact = Path::new(artifact);
    let cwd = Path::new(cwd);
    let expected = digest(&fs::read(artifact).unwrap());
    unsafe {
        libc::close(0);
        libc::close(1);
        libc::close(2);
    }
    let (controller, supervisor) = fixture_channel_pair(ControllerLimits {
        heartbeat_timeout: Duration::from_millis(200),
        frame_timeout: Duration::from_millis(200),
    })
    .unwrap();
    let mut controller = controller
        .spawn(
            SealedExecutable::read(artifact, expected).unwrap(),
            cwd,
            &[OsStr::new("controller-peer"), OsStr::new("normal")],
        )
        .unwrap();
    let original = controller.original_controller().unwrap();
    let mut supervisor = supervisor
        .bind_original(original)
        .unwrap()
        .spawn(
            SealedExecutable::read(artifact, expected).unwrap(),
            cwd,
            &[OsStr::new("supervisor-peer"), artifact.as_os_str()],
        )
        .unwrap();
    assert!(supervisor.wait_bounded(Duration::from_secs(3)).unwrap());
    assert!(controller.wait_bounded(Duration::from_secs(3)).unwrap());
    let text = String::from_utf8(supervisor.output().stdout.clone()).unwrap();
    assert!(text.contains(
        "direct-child-reaped=true;stdout-eof=true;stderr-eof=true;control-io-retired=true"
    ));
    assert!(
        controller
            .output()
            .stdout
            .starts_with(b"controller-tracked-status")
    );
    fs::write(receipt, b"typed-controller-closed-stdio-ok\n").unwrap();
}
