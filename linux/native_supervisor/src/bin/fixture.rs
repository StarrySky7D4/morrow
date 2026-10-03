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
        Some("orphan-guardian-peer") => orphan_guardian_peer(&args[1], &args[2]),
        Some("orphan-controller-peer") => orphan_controller_peer(&args[1], &args[2]),
        Some("orphan-supervisor-peer") => orphan_supervisor_peer(&args[1], &args[2]),
        Some("current-controller-peer") => current_controller_peer(&args[1], &args[2]),
        Some("current-supervisor-peer") => current_supervisor_peer(&args[1], &args[2]),
        Some("chain-dependent") => {
            let mut signal = 0;
            assert_eq!(
                unsafe { libc::prctl(libc::PR_GET_PDEATHSIG, &mut signal) },
                0
            );
            assert_eq!(signal, libc::SIGKILL);
            println!(
                "D pid={} parent={} pdeathsig={signal}",
                std::process::id(),
                unsafe { libc::getppid() }
            );
            eprintln!("D-stderr-ready");
            std::thread::sleep(Duration::from_secs(30));
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

// Actual parent-controller chain, restricted to this workspace-only fixture.
// C stays alive until it has exactly reaped S and observed both S output EOFs.
fn current_controller_peer(artifact: &str, mode: &str) {
    use morrow_linux_supervisor_foundation::{
        ControllerLimits, CurrentController, FrameKind, FrameRead, SealedExecutable,
        current_controller_fixture_pair, digest,
    };
    use std::{ffi::OsStr, fs, path::Path, time::Instant};
    let mut parent_signal = 0;
    assert_eq!(
        unsafe { libc::prctl(libc::PR_GET_PDEATHSIG, &mut parent_signal) },
        0
    );
    assert_eq!(parent_signal, libc::SIGKILL);
    let cwd = std::env::current_dir().unwrap();
    let sealed =
        SealedExecutable::read(Path::new(artifact), digest(&fs::read(artifact).unwrap())).unwrap();
    let current = CurrentController::capture().unwrap();
    assert_eq!(current.diagnostic_pid(), std::process::id());
    let launch = current_controller_fixture_pair(
        current,
        sealed,
        ControllerLimits {
            heartbeat_timeout: Duration::from_millis(200),
            frame_timeout: Duration::from_millis(200),
        },
    )
    .unwrap();
    if mode == "fork-reuse" {
        current_capture_fork_reuse(launch, &cwd, artifact);
        return;
    }
    let (mut transport, mut supervisor) = launch
        .spawn(
            &cwd,
            &[
                OsStr::new("current-supervisor-peer"),
                OsStr::new(artifact),
                OsStr::new(mode),
            ],
        )
        .unwrap();
    println!(
        "C pid={} parent={} S-owned-pid={} pdeathsig=9",
        std::process::id(),
        unsafe { libc::getppid() },
        supervisor.pid()
    );
    let started = Instant::now();
    let mut bootstrap = false;
    let mut replied = false;
    let mut pre_rejected = false;
    let mut no_replay_confirmed = false;
    let mut last_pulse = Duration::ZERO;
    let mut retired = false;
    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        while started.elapsed() < Duration::from_secs(4) {
            let now = started.elapsed();
            if !retired {
                transport.poll_write(now).unwrap();
                match transport.poll_read(now) {
                    Ok(FrameRead::Frame(frame)) => match frame.kind() {
                        FrameKind::Status if frame.payload() == b"bootstrap-ready" => {
                            assert!(!bootstrap);
                            bootstrap = true;
                            if mode == "pre-heartbeat" {
                                transport
                                    .queue(FrameKind::Data, b"discard-before-pulse", now)
                                    .unwrap();
                            } else {
                                transport.queue(FrameKind::Heartbeat, &[], now).unwrap();
                                transport
                                    .queue(FrameKind::Data, b"fresh-request", now)
                                    .unwrap();
                                last_pulse = now;
                            }
                        }
                        FrameKind::Diagnostic if frame.payload() == b"pre-heartbeat-rejected" => {
                            assert_eq!(mode, "pre-heartbeat");
                            pre_rejected = true;
                            transport.queue(FrameKind::Heartbeat, &[], now).unwrap();
                            last_pulse = now;
                        }
                        FrameKind::Status
                            if frame.payload() == b"heartbeat-decoded-no-dependent" =>
                        {
                            assert!(pre_rejected);
                            no_replay_confirmed = true;
                            transport
                                .queue(FrameKind::Data, b"fresh-request", now)
                                .unwrap();
                        }
                        FrameKind::Reply => {
                            assert_eq!(frame.payload(), b"fresh-request");
                            assert!(!replied);
                            replied = true;
                            if mode == "eof" {
                                transport.retire();
                                retired = true;
                            } else if mode == "normal" || mode == "pre-heartbeat" {
                                transport.queue(FrameKind::GracefulClose, &[], now).unwrap();
                            }
                        }
                        FrameKind::Status if frame.payload() == b"tracked-direct-cleanup-only" => {
                            assert!(replied);
                            transport.retire();
                            retired = true;
                        }
                        _ => panic!("unexpected authenticated supervisor frame"),
                    },
                    Ok(FrameRead::Eof) => {
                        transport.retire();
                        retired = true;
                    }
                    Ok(FrameRead::Pending) => (),
                    Err(error) => panic!("controller read: {error}"),
                }
                if !retired
                    && bootstrap
                    && !(mode == "pre-heartbeat" && !pre_rejected)
                    && !(mode == "expiry" && replied)
                    && now.saturating_sub(last_pulse) >= Duration::from_millis(80)
                {
                    transport.queue(FrameKind::Heartbeat, &[], now).unwrap();
                    last_pulse = now;
                }
            }
            supervisor.observe().unwrap();
            if supervisor.cleanup_complete() {
                assert!(replied);
                if mode == "pre-heartbeat" {
                    assert!(pre_rejected && no_replay_confirmed);
                }
                transport.retire();
                let text = String::from_utf8(supervisor.output().stdout.clone()).unwrap();
                assert!(text.contains("direct-child-reaped=true;stdout-eof=true;stderr-eof=true;control-io-retired=true"), "{text}");
                assert_eq!(
                    supervisor.exit_observation(),
                    Some(morrow_linux_supervisor_foundation::ExitObservation::Exited(
                        0
                    ))
                );
                print!("{text}");
                println!(
                    "C-live=true;S-exact-pidfd-reaped=true;S-stdout-eof={};S-stderr-eof={};pre-rejected={};no-replay-confirmed={};tree-empty=unproved;product-owner=false",
                    supervisor.output().stdout_eof,
                    supervisor.output().stderr_eof,
                    pre_rejected,
                    no_replay_confirmed
                );
                return;
            }
            std::thread::sleep(Duration::from_millis(1));
        }
        panic!("bounded current-controller fixture deadline");
    }));
    if let Err(failure) = outcome {
        transport.retire(); // Real EOF lets S terminate and reap D first.
        if !supervisor
            .wait_bounded(Duration::from_secs(2))
            .unwrap_or(false)
        {
            let _ = supervisor.terminate();
            let _ = supervisor.wait_bounded(Duration::from_secs(2));
        }
        std::panic::resume_unwind(failure);
    }
}

fn current_supervisor_peer(artifact: &str, mode: &str) {
    use morrow_linux_supervisor_foundation::{
        ControllerLoss, ExitObservation, FrameKind, FrameRead, OwnedProcess, SealedExecutable,
        SpawnFailure, take_supervisor_fixture_transport,
    };
    use std::{ffi::OsStr, path::Path, time::Instant};
    let (mut transport, mut watch) = unsafe { take_supervisor_fixture_transport() }.unwrap();
    assert_eq!(watch.diagnostic_pid(), unsafe { libc::getppid() } as u32);
    let mut parent_signal = 0;
    assert_eq!(
        unsafe { libc::prctl(libc::PR_GET_PDEATHSIG, &mut parent_signal) },
        0
    );
    assert_eq!(parent_signal, libc::SIGKILL);
    println!(
        "S pid={} parent={} original-C={} pdeathsig=9",
        std::process::id(),
        unsafe { libc::getppid() },
        watch.diagnostic_pid()
    );
    let cwd = std::env::current_dir().unwrap();
    let executable =
        SealedExecutable::read(Path::new(artifact), transport.bound_artifact_digest()).unwrap();
    let mut prepared = Some(
        OwnedProcess::prepare_controller_fixture(
            executable,
            &cwd,
            &[OsStr::new("chain-dependent")],
        )
        .unwrap(),
    );
    let started = Instant::now();
    transport
        .queue(FrameKind::Status, b"bootstrap-ready", started.elapsed())
        .unwrap();
    let mut child = None::<OwnedProcess>;
    let mut closing = false;
    let mut stopped = false;
    let mut replied = false;
    let mut denied = 0;
    let mut admitted = 0;
    let mut cleanup_status = false;
    let mut no_replay_acknowledged = false;
    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        while started.elapsed() < Duration::from_secs(3) {
            let now = started.elapsed();
            if !watch.poll() {
                closing = true;
            }
            if transport.healthy() {
                match transport.poll_read(now) {
                    Ok(FrameRead::Frame(frame)) => match frame.kind() {
                        FrameKind::Heartbeat => {
                            let accepted = watch.heartbeat(&frame).is_ok();
                            if accepted && denied == 1 && child.is_none() && !no_replay_acknowledged
                            {
                                no_replay_acknowledged = true;
                                // A discarded Data is never replayed by a heartbeat.
                                transport
                                    .queue(
                                        FrameKind::Status,
                                        b"heartbeat-decoded-no-dependent",
                                        now,
                                    )
                                    .unwrap();
                            }
                        }
                        FrameKind::Data if !closing && watch.poll() && !watch.admission_open() => {
                            assert!(child.is_none());
                            assert_eq!(frame.payload(), b"discard-before-pulse");
                            denied += 1;
                            transport
                                .queue(FrameKind::Diagnostic, b"pre-heartbeat-rejected", now)
                                .unwrap();
                        }
                        FrameKind::Data
                            if !closing
                                && watch.poll()
                                && watch.admission_open()
                                && child.is_none() =>
                        {
                            assert_eq!(frame.payload(), b"fresh-request");
                            match prepared.take().unwrap().spawn(&mut watch) {
                                Ok(owner) => child = Some(owner),
                                Err(SpawnFailure::Created { mut owner, error }) => {
                                    owner.terminate().unwrap();
                                    assert!(owner.wait_bounded(Duration::from_secs(2)).unwrap());
                                    panic!(
                                        "created child retained and cleaned after failure: {error}"
                                    );
                                }
                                Err(error) => panic!("dependent spawn: {error}"),
                            }
                            admitted += 1;
                        }
                        FrameKind::GracefulClose => closing = true,
                        _ => watch.revoke(ControllerLoss::Protocol),
                    },
                    Ok(FrameRead::Eof) => watch.revoke(ControllerLoss::TransportEof),
                    Err(_) => watch.revoke(ControllerLoss::Protocol),
                    Ok(FrameRead::Pending) => (),
                }
            }
            if watch.gate_closed() {
                closing = true;
            }
            if transport.healthy() && transport.poll_write(started.elapsed()).is_err() {
                watch.revoke(ControllerLoss::Backpressure);
                closing = true;
            }
            if let Some(child) = child.as_mut() {
                child.observe().unwrap();
                if !replied && !closing {
                    let ready = format!(
                        "D pid={} parent={} pdeathsig=9\n",
                        child.pid(),
                        std::process::id()
                    );
                    if child.output().stdout == ready.as_bytes()
                        && child.output().stderr == b"D-stderr-ready\n"
                    {
                        print!("{ready}");
                        transport
                            .queue(FrameKind::Reply, b"fresh-request", started.elapsed())
                            .unwrap();
                        replied = true;
                    }
                }
                if closing && !stopped {
                    child.terminate().unwrap();
                    stopped = true;
                }
                if closing && child.cleanup_complete() {
                    assert_eq!(admitted, 1);
                    assert!(replied);
                    assert_eq!(
                        child.exit_observation(),
                        Some(ExitObservation::Signaled(libc::SIGKILL))
                    );
                    if !watch.gate_closed() && transport.healthy() && !cleanup_status {
                        transport
                            .queue(
                                FrameKind::Status,
                                b"tracked-direct-cleanup-only",
                                started.elapsed(),
                            )
                            .unwrap();
                        cleanup_status = true;
                        continue;
                    }
                    if watch.gate_closed() || !transport.healthy() || transport.queued_bytes() == 0
                    {
                        let first_loss = watch.loss();
                        if first_loss.is_some() {
                            watch.revoke(ControllerLoss::Protocol);
                            assert_eq!(watch.loss(), first_loss);
                        }
                        if mode == "expiry" {
                            assert_eq!(
                                first_loss.unwrap().reason,
                                ControllerLoss::HeartbeatExpired
                            );
                        }
                        if mode == "eof" {
                            assert_eq!(first_loss.unwrap().reason, ControllerLoss::TransportEof);
                        }
                        if mode == "pre-heartbeat" {
                            assert_eq!(denied, 1);
                        }
                        transport.retire();
                        println!(
                            "loss={:?};sticky-first-loss={};data-admitted={};pre-heartbeat-denied={};direct-child-reaped=true;stdout-eof=true;stderr-eof=true;control-io-retired={};tree-empty=unproved;product-owner=false",
                            first_loss.map(|l| l.reason),
                            first_loss.is_some(),
                            admitted,
                            denied,
                            transport.io_retired()
                        );
                        return;
                    }
                }
            }
            std::thread::sleep(Duration::from_millis(1));
        }
        transport.retire();
        if let Some(child) = child.as_mut() {
            child.terminate().unwrap();
            assert!(child.wait_bounded(Duration::from_secs(2)).unwrap());
        }
        panic!("bounded current-supervisor deadline");
    }));
    if let Err(failure) = outcome {
        transport.retire();
        if let Some(child) = child.as_mut() {
            let _ = child.terminate();
            let _ = child.wait_bounded(Duration::from_secs(2));
        }
        std::panic::resume_unwind(failure);
    }
}

fn current_capture_fork_reuse(
    launch: morrow_linux_supervisor_foundation::CurrentControllerFixtureLaunch,
    cwd: &std::path::Path,
    artifact: &str,
) {
    use morrow_linux_supervisor_foundation::SpawnFailure;
    use std::{
        ffi::OsStr,
        os::fd::{FromRawFd, OwnedFd},
    };
    // Isolated single-threaded fixture. CLONE_PIDFD grants C exact reaping
    // authority over this fork witness; it is not authority over another PID.
    let mut fd = -1;
    let pid = unsafe {
        libc::syscall(
            libc::SYS_clone,
            (libc::CLONE_PIDFD | libc::SIGCHLD) as libc::c_ulong,
            0usize,
            &mut fd as *mut i32,
            0usize,
            0usize,
        )
    };
    assert!(pid >= 0);
    if pid == 0 {
        let result = launch.spawn(
            cwd,
            &[
                OsStr::new("current-supervisor-peer"),
                OsStr::new(artifact),
                OsStr::new("normal"),
            ],
        );
        let rejected = match result {
            Err(SpawnFailure::NotCreated(error)) => {
                error.to_string().contains("capture process changed")
            }
            Err(SpawnFailure::Created { mut owner, .. }) => {
                let _ = owner.terminate();
                let _ = owner.wait_bounded(Duration::from_secs(2));
                false
            }
            Ok((_, mut owner)) => {
                let _ = owner.terminate();
                let _ = owner.wait_bounded(Duration::from_secs(2));
                false
            }
        };
        // No S was created and no copied endpoint escaped this rejection.
        unsafe {
            libc::_exit(if rejected { 0 } else { 71 });
        }
    }
    let mut witness = ForkWitness {
        fd: unsafe { OwnedFd::from_raw_fd(fd) },
        reaped: false,
    };
    let info = witness
        .wait_bounded(Duration::from_secs(2))
        .expect("bounded fork witness reaping");
    assert_eq!(unsafe { info.si_pid() }, pid as i32);
    assert_eq!(info.si_code, libc::CLD_EXITED);
    assert_eq!(unsafe { info.si_status() }, 0);
    println!(
        "fork-reuse-refused-before-supervisor-launch=true;witness-pid={pid};witness-exact-pidfd-reaped=true;no-dependent-child"
    );
}

// Failure cleanup retains the exact clone-created pidfd, never a numeric kill.
struct ForkWitness {
    fd: std::os::fd::OwnedFd,
    reaped: bool,
}
impl ForkWitness {
    fn wait_bounded(&mut self, timeout: Duration) -> Option<libc::siginfo_t> {
        use std::os::fd::AsRawFd;
        let start = std::time::Instant::now();
        while start.elapsed() < timeout {
            let mut info: libc::siginfo_t = unsafe { std::mem::zeroed() };
            let result = unsafe {
                libc::waitid(
                    libc::P_PIDFD,
                    self.fd.as_raw_fd() as u32,
                    &mut info,
                    libc::WEXITED | libc::WNOHANG,
                )
            };
            if result == 0 && unsafe { info.si_pid() } != 0 {
                self.reaped = true;
                return Some(info);
            }
            if result < 0 && io::Error::last_os_error().kind() != io::ErrorKind::Interrupted {
                return None;
            }
            std::thread::sleep(Duration::from_millis(1));
        }
        None
    }
}
impl Drop for ForkWitness {
    fn drop(&mut self) {
        use std::os::fd::AsRawFd;
        if !self.reaped {
            unsafe {
                libc::syscall(
                    libc::SYS_pidfd_send_signal,
                    self.fd.as_raw_fd(),
                    libc::SIGKILL,
                    std::ptr::null::<libc::siginfo_t>(),
                    0u32,
                );
            }
            let _ = self.wait_bounded(Duration::from_secs(2));
        }
    }
}

// This G is a fresh, dedicated subprocess. No outer harness/host attribute or
// SIGCHLD handler is changed, and setup refusal occurs before C/S/D creation.
fn orphan_guardian_peer(artifact: &str, mode: &str) {
    use morrow_linux_supervisor_foundation::{
        ControllerLimits, CurrentController, ExitObservation, GuardianEvent,
        GuardianWaitEligibility, SealedExecutable, digest, guardian_controller_fixture_launch,
    };
    use std::{ffi::OsStr, fs, path::Path, time::Instant};
    let mut before = 0;
    assert_eq!(
        unsafe { libc::prctl(libc::PR_GET_CHILD_SUBREAPER, &mut before) },
        0
    );
    assert_eq!(before, 0);
    if mode == "setup-reject" {
        println!(
            "setup-error-injected=true;subreaper-before=0;no-C-no-S-no-D=true;no-deliberate-controller-death=true"
        );
        return;
    }
    assert_eq!(
        unsafe { libc::prctl(libc::PR_SET_CHILD_SUBREAPER, 1) },
        0,
        "process-local guardian setup refused; no workaround"
    );
    let mut after = 0;
    assert_eq!(
        unsafe { libc::prctl(libc::PR_GET_CHILD_SUBREAPER, &mut after) },
        0
    );
    assert_eq!(after, 1);
    let cwd = std::env::current_dir().unwrap();
    let expected = digest(&fs::read(artifact).unwrap());
    let launch = guardian_controller_fixture_launch(
        CurrentController::capture().unwrap(),
        SealedExecutable::read(Path::new(artifact), expected).unwrap(),
        ControllerLimits {
            heartbeat_timeout: Duration::from_millis(200),
            frame_timeout: Duration::from_millis(200),
        },
    )
    .unwrap();
    let (mut witness, mut controller) = launch
        .spawn(
            &cwd,
            &[
                OsStr::new("orphan-controller-peer"),
                OsStr::new(artifact),
                OsStr::new(mode),
            ],
        )
        .unwrap();
    let started = Instant::now();
    let negative = mode.starts_with("registration-");
    let mut registration = false;
    let mut rejected = false;
    let mut ack = false;
    let mut armed = false;
    let mut queued = false;
    let mut dependent = None;
    let mut killed = false;
    let mut cleaned = false;
    let mut finished = false;
    let mut holder = false;
    let mut adopted = false;
    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        while started.elapsed() < Duration::from_secs(7) {
            if (!ack || !queued) && !(negative && ack) {
                match witness.poll_controller() {
                    Ok(Some(GuardianEvent::Registered)) => {
                        assert!(!negative);
                        registration = true;
                        assert_eq!(
                            witness.wait_eligibility().unwrap(),
                            GuardianWaitEligibility::NotChild
                        );
                        println!(
                            "G pid={} parent={} subreaper=1 C-owned={} S-registered={};pre-adoption-wait=ECHILD",
                            std::process::id(),
                            unsafe { libc::getppid() },
                            controller.pid(),
                            witness.supervisor_pid().unwrap()
                        );
                    }
                    Ok(Some(GuardianEvent::LateQueued)) => queued = true,
                    Ok(None) => (),
                    Err(error) if negative && !rejected => {
                        rejected = true;
                        println!(
                            "registration-refused={mode};reason={error};C-still-live=true;no-deliberate-controller-death=true"
                        );
                    }
                    other => panic!(
                        "unexpected C registration event: {}",
                        matches!(other, Err(_))
                    ),
                }
            }
            if (registration || rejected) && !ack {
                ack = witness.acknowledge_registration(!rejected).unwrap();
            }
            if negative {
                controller.observe().unwrap();
                if controller.cleanup_complete() {
                    assert!(rejected && ack);
                    assert_eq!(
                        controller.exit_observation(),
                        Some(ExitObservation::Exited(0))
                    );
                    let out = String::from_utf8(controller.output().stdout.clone()).unwrap();
                    assert!(out.contains("S-live-C-exact-reaped=true;S-stdout-eof=true;S-stderr-eof=true;no-dependent-child"),"{out}");
                    print!("{out}");
                    println!(
                        "G-exact-C-reaped=true;C-stdout-eof=true;C-stderr-eof=true;registration-negative=true;tree-empty=unproved;product-owner=false"
                    );
                    return;
                }
                std::thread::sleep(Duration::from_millis(1));
                continue;
            }
            if registration && !cleaned {
                match witness.poll_supervisor().unwrap() {
                    Some(GuardianEvent::DependentReady {
                        supervisor,
                        controller: c,
                        dependent: d,
                        guardian,
                    }) => {
                        assert_eq!(c, controller.pid());
                        assert_eq!(Some(supervisor), witness.supervisor_pid());
                        assert_eq!(guardian, std::process::id());
                        dependent = Some(d);
                    }
                    Some(GuardianEvent::Armed) => armed = true,
                    Some(GuardianEvent::RetainedWriter { stderr }) => {
                        assert_eq!(stderr, mode == "hold-stderr");
                        assert!(mode == "hold-stderr" || mode == "hold-stdout");
                        holder = true;
                    }
                    Some(GuardianEvent::Cleaned {
                        supervisor,
                        parent,
                        controller: c,
                        dependent: d,
                        late_heartbeat,
                        late_data,
                    }) => {
                        assert!(killed && armed && queued);
                        assert_eq!(Some(supervisor), witness.supervisor_pid());
                        assert_eq!(parent, std::process::id());
                        assert_eq!(c, controller.pid());
                        assert_eq!(Some(d), dependent);
                        assert_eq!((late_heartbeat, late_data), (1, 1));
                        assert_eq!(
                            witness.wait_eligibility().unwrap(),
                            GuardianWaitEligibility::LiveChild
                        );
                        adopted = true;
                        cleaned = true;
                        println!(
                            "post-adoption-live-wait=true;S-parent=G;original-C-loss=OriginalProcessExited;late-heartbeat-rejected=1;late-data-denied=1;D-exact-reaped=true;D-both-eofs=true;S-still-live=true"
                        );
                    }
                    None => (),
                    _ => panic!("unexpected S report"),
                }
            }
            if registration && ack && dependent.is_some() && armed && queued && !killed {
                controller.terminate().unwrap();
                killed = true;
            }
            if killed {
                controller.observe().unwrap();
            }
            if cleaned && !finished {
                finished = witness.acknowledge_finish().unwrap();
            }
            if adopted {
                let complete = witness.observe_supervisor().unwrap();
                if holder && witness.supervisor_exit().is_some() {
                    let output = witness.supervisor_output().unwrap();
                    assert!(!complete);
                    let other_eof = if mode == "hold-stdout" {
                        assert!(!output.stdout_eof);
                        output.stderr_eof
                    } else {
                        assert!(!output.stderr_eof);
                        output.stdout_eof
                    };
                    if other_eof {
                        println!(
                            "G-owned-{mode}-writer=true;S-exact-reaped-before-output-release=true;cleanup-with-one-missing-eof=false;stdout-eof={};stderr-eof={}",
                            output.stdout_eof, output.stderr_eof
                        );
                        witness.release_output_witness();
                        holder = false;
                    }
                }
                if complete && controller.cleanup_complete() {
                    assert_eq!(
                        controller.exit_observation(),
                        Some(ExitObservation::Signaled(libc::SIGKILL))
                    );
                    assert_eq!(witness.supervisor_exit(), Some(ExitObservation::Exited(0)));
                    let out =
                        String::from_utf8(witness.supervisor_output().unwrap().stdout.clone())
                            .unwrap();
                    let c = String::from_utf8(controller.output().stdout.clone()).unwrap();
                    print!("{c}{out}");
                    assert!(out.contains("OriginalProcessExited;sticky=true;data-admitted=1;late-heartbeat-rejected=1;late-data-denied=1;D-exact-reaped=true;D-stdout-eof=true;D-stderr-eof=true;control-io-retired=true"),"{out}");
                    println!(
                        "G-exact-C-reaped=true;C-stdout-eof=true;C-stderr-eof=true;G-exact-adopted-S-reaped=true;S-stdout-eof=true;S-stderr-eof=true;passive-C-writer-holder=G;tree-empty=unproved;product-owner=false"
                    );
                    return;
                }
            }
            std::thread::sleep(Duration::from_millis(1));
        }
        panic!("bounded guardian fixture deadline");
    }));
    if let Err(failure) = outcome {
        // Retain exact objects for bounded failure cleanup; no success receipt.
        let _ = controller.terminate();
        let _ = controller.wait_bounded(Duration::from_secs(2));
        witness.release_output_witness();
        witness.release_passive_control_writer();
        let _ = witness.acknowledge_finish();
        let until = Instant::now();
        while until.elapsed() < Duration::from_secs(3) {
            if matches!(
                witness.wait_eligibility(),
                Ok(GuardianWaitEligibility::LiveChild | GuardianWaitEligibility::ExitedUnreaped)
            ) && witness.observe_supervisor().unwrap_or(false)
            {
                break;
            }
            std::thread::sleep(Duration::from_millis(1));
        }
        if witness.supervisor_exit().is_none() {
            let _ = witness.signal_registered_supervisor();
        }
        // S failure/emergency and unresponsive kernel cleanup are unqualified.
        std::panic::resume_unwind(failure);
    }
}
fn orphan_controller_peer(artifact: &str, mode: &str) {
    use morrow_linux_supervisor_foundation::{
        CurrentController, FrameKind, FrameRead, GuardianRegistrationFault, SealedExecutable,
        digest, take_guardian_controller_fixture,
    };
    use std::{ffi::OsStr, fs, path::Path, time::Instant};
    let mut signal = 0;
    assert_eq!(
        unsafe { libc::prctl(libc::PR_GET_PDEATHSIG, &mut signal) },
        0
    );
    assert_eq!(signal, libc::SIGKILL);
    let mut witness = unsafe { take_guardian_controller_fixture() }.unwrap();
    let cwd = std::env::current_dir().unwrap();
    let executable =
        SealedExecutable::read(Path::new(artifact), digest(&fs::read(artifact).unwrap())).unwrap();
    let (mut transport, mut supervisor) = witness
        .spawn_supervisor(
            CurrentController::capture().unwrap(),
            executable,
            &cwd,
            &[
                OsStr::new("orphan-supervisor-peer"),
                OsStr::new(artifact),
                OsStr::new(mode),
            ],
        )
        .unwrap();
    if mode == "fork-reclaim" {
        fork_reclaim_refusal(&mut supervisor);
    }
    println!(
        "C2 pid={} parent={} S-owned={};ordinary-C-pdeathsig=9",
        std::process::id(),
        unsafe { libc::getppid() },
        supervisor.pid()
    );
    let fault = match mode {
        "registration-missing" => Some(GuardianRegistrationFault::Missing),
        "registration-extra" => Some(GuardianRegistrationFault::Extra),
        "registration-truncated" => Some(GuardianRegistrationFault::Truncated),
        "registration-role" => Some(GuardianRegistrationFault::WrongRole),
        "registration-replay" => Some(GuardianRegistrationFault::Replay),
        _ => None,
    };
    let started = Instant::now();
    let mut registered = false;
    let mut accepted = false;
    let mut ready = false;
    let mut late = false;
    let mut queued = false;
    let mut last = Duration::ZERO;
    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        while started.elapsed() < Duration::from_secs(6) {
            if !registered {
                registered = witness
                    .register_supervisor(&supervisor, &transport, fault)
                    .unwrap();
            }
            if registered && !accepted {
                if let Some(allow) = witness.poll_ack().unwrap() {
                    if !allow {
                        assert!(fault.is_some());
                        transport.retire();
                        assert!(supervisor.reclaim_while_controller_live().unwrap());
                        assert!(supervisor.output().stdout_eof && supervisor.output().stderr_eof);
                        println!(
                            "S-live-C-exact-reaped=true;S-stdout-eof=true;S-stderr-eof=true;no-dependent-child"
                        );
                        return;
                    }
                    assert!(fault.is_none());
                    accepted = true;
                }
            }
            let now = started.elapsed();
            transport.poll_write(now).unwrap();
            if accepted && !late {
                match transport.poll_read(now).unwrap() {
                    FrameRead::Frame(frame)
                        if frame.kind() == FrameKind::Status
                            && frame.payload() == b"bootstrap-ready" =>
                    {
                        ready = true;
                        transport.queue(FrameKind::Heartbeat, &[], now).unwrap();
                        transport
                            .queue(FrameKind::Data, b"orphan-dependent", now)
                            .unwrap();
                        last = now;
                    }
                    FrameRead::Frame(frame) if frame.kind() == FrameKind::Reply => {
                        assert_eq!(frame.payload(), b"orphan-dependent");
                        // A real decoded arm Data fence makes ordinary periodic
                        // heartbeats incapable of accidentally arming this phase.
                        transport
                            .queue(FrameKind::Data, b"arm-original-object-witness", now)
                            .unwrap();
                        // Next pulse is decoded/arms S; the following HB/Data
                        // remain genuinely queued until original C exits.
                        transport.queue(FrameKind::Heartbeat, &[], now).unwrap();
                        transport.queue(FrameKind::Heartbeat, &[], now).unwrap();
                        transport
                            .queue(FrameKind::Data, b"late-must-not-create", now)
                            .unwrap();
                        late = true;
                    }
                    FrameRead::Pending => (),
                    _ => panic!("unexpected orphan control frame"),
                }
                if ready && !late && now.saturating_sub(last) >= Duration::from_millis(80) {
                    transport.queue(FrameKind::Heartbeat, &[], now).unwrap();
                    last = now;
                }
            }
            if late && !queued && transport.queued_bytes() == 0 {
                queued = witness.late_queued().unwrap();
            }
            // Do not read shared S output or reap/drop S after acknowledged
            // registration. Successful case intentionally ends by exact SIGKILL.
            std::thread::sleep(Duration::from_millis(1));
        }
        panic!("C was not deliberately collected within witness deadline");
    }));
    if let Err(failure) = outcome {
        transport.retire();
        let _ = supervisor.reclaim_while_controller_live();
        std::panic::resume_unwind(failure);
    }
}
fn orphan_supervisor_peer(artifact: &str, mode: &str) {
    use morrow_linux_supervisor_foundation::{
        ControllerLoss, ExitObservation, FrameKind, FrameRead, OwnedProcess, SealedExecutable,
        SpawnFailure, take_guardian_supervisor_fixture, take_supervisor_fixture_transport,
    };
    use std::{ffi::OsStr, path::Path, time::Instant};
    let mut witness = unsafe { take_guardian_supervisor_fixture() }.unwrap();
    let (mut transport, mut watch) = unsafe { take_supervisor_fixture_transport() }.unwrap();
    let c = watch.diagnostic_pid();
    let g = witness.guardian_pid();
    assert_eq!(unsafe { libc::getppid() } as u32, c);
    let mut signal = -1;
    assert_eq!(
        unsafe { libc::prctl(libc::PR_GET_PDEATHSIG, &mut signal) },
        0
    );
    assert_eq!(signal, 0);
    println!(
        "S2 pid={} initial-parent={} original-C={} G={} S-only-pdeathsig=0",
        std::process::id(),
        c,
        c,
        g
    );
    let cwd = std::env::current_dir().unwrap();
    let executable =
        SealedExecutable::read(Path::new(artifact), transport.bound_artifact_digest()).unwrap();
    let mut prepared = Some(
        OwnedProcess::prepare_controller_fixture(
            executable,
            &cwd,
            &[OsStr::new("chain-dependent")],
        )
        .unwrap(),
    );
    let started = Instant::now();
    transport
        .queue(FrameKind::Status, b"bootstrap-ready", started.elapsed())
        .unwrap();
    let mut child = None::<OwnedProcess>;
    let mut ready = false;
    let mut replied = false;
    let mut armed = false;
    let mut arm_requested = false;
    let mut arm_sent = false;
    let mut hold_sent = !(mode == "hold-stdout" || mode == "hold-stderr");
    let mut late_hb = 0;
    let mut late_data = 0;
    let mut report = false;
    let mut stopped = false;
    let mut cleaned = false;
    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        while started.elapsed() < Duration::from_secs(5) {
            let now = started.elapsed();
            let live = watch.poll();
            if !hold_sent {
                hold_sent = witness.retain_output_writer(mode == "hold-stderr").unwrap();
            }
            if live && !armed {
                match transport.poll_read(now).unwrap() {
                    FrameRead::Frame(frame) => match frame.kind() {
                        FrameKind::Heartbeat => {
                            watch.heartbeat(&frame).unwrap();
                            if replied && arm_requested {
                                armed = true;
                            }
                        }
                        FrameKind::Data if watch.admission_open() && child.is_none() => {
                            assert_eq!(frame.payload(), b"orphan-dependent");
                            match prepared.take().unwrap().spawn(&mut watch) {
                                Ok(owner) => child = Some(owner),
                                Err(SpawnFailure::Created { mut owner, error }) => {
                                    owner.terminate().unwrap();
                                    assert!(owner.wait_bounded(Duration::from_secs(2)).unwrap());
                                    panic!("created orphan dependent reclaimed: {error}");
                                }
                                Err(error) => panic!("orphan dependent: {error}"),
                            }
                        }
                        FrameKind::Data
                            if replied
                                && child.is_some()
                                && watch.admission_open()
                                && frame.payload() == b"arm-original-object-witness" =>
                        {
                            assert!(!arm_requested);
                            arm_requested = true;
                        }
                        _ => panic!("unexpected pre-arm business frame"),
                    },
                    FrameRead::Pending => (),
                    FrameRead::Eof => {
                        assert!(mode.starts_with("registration-"));
                        transport.retire();
                        assert!(child.is_none());
                        return;
                    }
                }
            }
            if live {
                transport.poll_write(started.elapsed()).unwrap();
            }
            if let Some(child) = child.as_mut() {
                child.observe().unwrap();
                if !ready {
                    let line = format!(
                        "D pid={} parent={} pdeathsig=9\n",
                        child.pid(),
                        std::process::id()
                    );
                    if child.output().stdout == line.as_bytes()
                        && child.output().stderr == b"D-stderr-ready\n"
                    {
                        if witness.dependent_ready(child.pid(), c).unwrap() {
                            print!("{line}");
                            ready = true;
                        }
                    }
                }
                if ready && !replied {
                    transport
                        .queue(FrameKind::Reply, b"orphan-dependent", started.elapsed())
                        .unwrap();
                    replied = true;
                }
                if armed && !arm_sent {
                    arm_sent = witness.armed().unwrap();
                }
                if !live && !stopped {
                    let first = watch.loss().unwrap();
                    assert_eq!(
                        first.reason,
                        ControllerLoss::OriginalProcessExited,
                        "real first cause must not be overwritten"
                    );
                    // The real passive writer held by G keeps these queued bytes
                    // readable without creating EOF or adding any business frame.
                    for _ in 0..4 {
                        match transport.poll_read(started.elapsed()).unwrap() {
                            FrameRead::Frame(frame) if frame.kind() == FrameKind::Heartbeat => {
                                assert!(watch.heartbeat(&frame).is_err());
                                late_hb += 1;
                            }
                            FrameRead::Frame(frame) if frame.kind() == FrameKind::Data => {
                                assert!(!watch.admission_open());
                                assert_eq!(frame.payload(), b"late-must-not-create");
                                late_data += 1;
                            }
                            FrameRead::Pending => break,
                            _ => panic!("unexpected late frame/EOF"),
                        }
                    }
                    assert_eq!((late_hb, late_data), (1, 1));
                    watch.revoke(ControllerLoss::Protocol);
                    assert_eq!(watch.loss(), Some(first));
                    transport.retire();
                    child.terminate().unwrap();
                    stopped = true;
                }
                if stopped && child.cleanup_complete() && !cleaned {
                    assert_eq!(
                        child.exit_observation(),
                        Some(ExitObservation::Signaled(libc::SIGKILL))
                    );
                    assert!(child.output().stdout_eof && child.output().stderr_eof);
                    cleaned = true;
                    println!(
                        "OriginalProcessExited;sticky=true;data-admitted=1;late-heartbeat-rejected=1;late-data-denied=1;D-exact-reaped=true;D-stdout-eof=true;D-stderr-eof=true;control-io-retired={};tree-empty=unproved;product-owner=false",
                        transport.io_retired()
                    );
                }
                if cleaned && unsafe { libc::getppid() } as u32 == g && !report {
                    report = witness.cleaned(child.pid(), c, late_hb, late_data).unwrap();
                }
                if report && witness.poll_finish().unwrap() == Some(true) {
                    return;
                }
            } else if !live {
                assert!(mode.starts_with("registration-"));
                transport.retire();
                return;
            }
            std::thread::sleep(Duration::from_millis(1));
        }
        panic!("bounded orphan supervisor deadline");
    }));
    if let Err(failure) = outcome {
        transport.retire();
        if let Some(child) = child.as_mut() {
            let _ = child.terminate();
            let _ = child.wait_bounded(Duration::from_secs(2));
        }
        std::panic::resume_unwind(failure);
    }
}

// Genuine fork-context misuse in an isolated single-threaded C fixture. The
// copied wrapper is never dropped in the child; _exit avoids an unrelated
// destructor/backstop path. Parent C retains and later hands off the actual S.
fn fork_reclaim_refusal(
    supervisor: &mut morrow_linux_supervisor_foundation::GuardianSupervisorProcess,
) {
    use std::os::fd::{FromRawFd, OwnedFd};
    let mut fd = -1;
    let pid = unsafe {
        libc::syscall(
            libc::SYS_clone,
            (libc::CLONE_PIDFD | libc::SIGCHLD) as libc::c_ulong,
            0usize,
            &mut fd as *mut i32,
            0usize,
            0usize,
        )
    };
    assert!(pid >= 0);
    if pid == 0 {
        let refused = supervisor
            .reclaim_while_controller_live()
            .err()
            .is_some_and(|e| e.to_string().contains("process changed before reclaim"));
        unsafe {
            libc::_exit(if refused { 0 } else { 73 });
        }
    }
    let mut witness = ForkWitness {
        fd: unsafe { OwnedFd::from_raw_fd(fd) },
        reaped: false,
    };
    let info = witness
        .wait_bounded(Duration::from_secs(2))
        .expect("bounded reclaim fork witness reap");
    assert_eq!(unsafe { info.si_pid() }, pid as i32);
    assert_eq!(info.si_code, libc::CLD_EXITED);
    assert_eq!(unsafe { info.si_status() }, 0);
    println!(
        "fork-reclaim-refused-before-signal=true;fork-witness-pid={pid};fork-witness-exact-reaped=true;real-S-retained-by-C=true"
    );
}
