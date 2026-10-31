//! Explicit native fixture, never part of broad library-test discovery.
//! Only a newly created per-invocation directory is mutated.
use morrow_fs_watch_native_v1::windows::{Fault, NativeWatch, Poll, Window};
use morrow_fs_watch_v1::{
    Batch, End, Kind,
    state::{ClientState, Limits, Phase},
    wire_digest,
};
use std::{
    fs::{self, File, OpenOptions},
    io::Write,
    os::windows::{fs::OpenOptionsExt, io::AsRawHandle},
    path::{Path, PathBuf},
    time::Duration,
};
use windows_sys::Win32::Foundation::{
    GetHandleInformation, HANDLE_FLAG_PROTECT_FROM_CLOSE, SetHandleInformation,
};

#[derive(Debug)]
enum Failure {
    Check(&'static str),
    Native(Fault),
    Io(std::io::ErrorKind, Option<i32>),
    Codec,
}
impl From<Fault> for Failure {
    fn from(value: Fault) -> Self {
        Self::Native(value)
    }
}
impl From<std::io::Error> for Failure {
    fn from(value: std::io::Error) -> Self {
        Self::Io(value.kind(), value.raw_os_error())
    }
}
impl From<morrow_fs_watch_v1::Error> for Failure {
    fn from(_: morrow_fs_watch_v1::Error) -> Self {
        Self::Codec
    }
}
impl std::fmt::Display for Failure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Check(s) => write!(f, "check {s}"),
            Self::Native(n) => write!(f, "native {n}"),
            Self::Io(e, code) => write!(f, "io {e:?} native_code={code:?}"),
            Self::Codec => write!(f, "codec"),
        }
    }
}
fn check(ok: bool, reason: &'static str) -> Result<(), Failure> {
    if ok {
        Ok(())
    } else {
        Err(Failure::Check(reason))
    }
}
fn retain_unknown(stage: &str) -> ! {
    eprintln!("NATIVE_WATCH_UNKNOWN_RETAINED_DO_NOT_POLL_OR_REPLAY {stage}");
    loop {
        std::thread::park();
    }
}
fn name(text: &str) -> Vec<u8> {
    text.encode_utf16().flat_map(u16::to_le_bytes).collect()
}

struct Observation {
    client: ClientState,
    frames: u32,
    kinds: u32,
    wrong_ack_checked: bool,
}
impl Observation {
    fn new() -> Result<Self, Failure> {
        Ok(Self {
            client: ClientState::new(
                [0x42; 32],
                Limits {
                    frames: 64,
                    changes: 4096,
                    wire_bytes: 1024 * 1024,
                },
            )?,
            frames: 0,
            kinds: 0,
            wrong_ack_checked: false,
        })
    }
    fn accept(
        &mut self,
        watch: &mut NativeWatch,
        evidence: &Path,
        sequence: u64,
        wire: Vec<u8>,
        acknowledge: bool,
    ) -> Result<Batch, Failure> {
        let value = self.client.receive(&wire)?;
        check(
            value.sequence == sequence && value.watch_epoch == [0x42; 32],
            "wire correlation",
        )?;
        self.frames += 1;
        for change in &value.changes {
            self.kinds |= 1u32 << change.kind as u16;
        }
        let mut saved = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(evidence.join(format!("frame-{:03}.capnp", self.frames)))?;
        saved.write_all(&wire)?;
        saved.sync_all()?;
        drop(saved);
        if acknowledge {
            if !self.wrong_ack_checked {
                let before = watch.snapshot();
                check(
                    watch.acknowledge(sequence + 1, wire_digest(&wire)) == Err(Fault::Correlation),
                    "wrong sequence ACK refused",
                )?;
                check(
                    watch.acknowledge(sequence, [0; 32]) == Err(Fault::Correlation),
                    "wrong wire ACK refused",
                )?;
                check(
                    watch.arm() == Err(Fault::WrongState),
                    "pending frame blocks next read",
                )?;
                check(
                    watch.snapshot() == before,
                    "wrong ACK did not mutate original owner",
                )?;
                self.wrong_ack_checked = true;
            }
            self.client.acknowledge(sequence, wire_digest(&wire))?;
            watch.acknowledge(sequence, wire_digest(&wire))?;
            let before = watch.snapshot();
            check(
                watch.acknowledge(sequence, wire_digest(&wire)) == Err(Fault::WrongState),
                "duplicate ACK refused",
            )?;
            check(
                watch.snapshot() == before,
                "duplicate ACK did not mutate original owner",
            )?;
        }
        Ok(value)
    }
}

fn collect(
    watch: &mut NativeWatch,
    observed: &mut Observation,
    evidence: &Path,
    wanted: &[(Kind, Vec<u8>)],
) -> Result<(), Failure> {
    let mut found = vec![false; wanted.len()];
    loop {
        match watch.poll(Duration::from_millis(50))? {
            Poll::Pending => {}
            Poll::DeadlineReached => {
                return Err(Failure::Check("original observation deadline reached"));
            }
            Poll::Frame { sequence, wire } => {
                let value = observed.accept(watch, evidence, sequence, wire, true)?;
                check(
                    value.end == End::Continue,
                    "unexpected terminal observation",
                )?;
                for (index, (kind, name)) in wanted.iter().enumerate() {
                    found[index] |= value
                        .changes
                        .iter()
                        .any(|x| x.kind == *kind && x.name_utf16_le == *name);
                }
                if found.iter().all(|v| *v) {
                    return Ok(());
                }
                watch.arm()?;
            }
        }
    }
}

fn scenario(
    mode: &str,
    fixture: &Path,
    evidence: &Path,
    watch: &mut NativeWatch,
    observed: &mut Observation,
) -> Result<(), Failure> {
    match mode {
        "actual-overflow" => {
            // Keep the original read armed while a bounded burst exceeds the
            // handle's notification buffer. No fabricated notification enters it.
            watch.arm()?;
            for index in 0..4096 {
                fs::create_dir(fixture.join(format!(
                    "gap-{index:04}-abcdefghijklmnopqrstuvwxyz0123456789"
                )))?;
            }
            println!("NATIVE_WATCH_ACTUAL_BURST directories=4096");
            for _ in 0..4 {
                let value = loop {
                    match watch.poll(Duration::from_millis(50))? {
                        Poll::Pending => (),
                        Poll::DeadlineReached => {
                            return Err(Failure::Check("original overflow deadline"));
                        }
                        Poll::Frame { sequence, wire } => {
                            break observed.accept(watch, evidence, sequence, wire, true)?;
                        }
                    }
                };
                if value.end == End::Gap {
                    check(value.changes.is_empty(), "gap contains no invented changes")?;
                    check(
                        observed.client.phase() == Phase::Ended(End::Gap),
                        "actual gap ends the original consumer",
                    )?;
                    let before = watch.snapshot();
                    check(watch.arm() == Err(Fault::WrongState), "gap cannot restart")?;
                    check(
                        watch.snapshot() == before,
                        "gap refusal made no native call",
                    )?;
                    println!("NATIVE_WATCH_ACTUAL_GAP sequence={}", value.sequence);
                    return Ok(());
                }
                check(
                    value.end == End::Continue,
                    "unexpected burst terminal state",
                )?;
                watch.arm()?;
            }
            return Err(Failure::Check(
                "bounded burst did not establish an actual gap",
            ));
        }
        "events" => {
            let old = "雪 😀.txt";
            let new = "renamed 😀.txt";
            watch.arm()?;
            fs::write(fixture.join(old), b"first native fixture bytes")?;
            collect(watch, observed, evidence, &[(Kind::Added, name(old))])?;
            watch.arm()?;
            let mut append = OpenOptions::new().append(true).open(fixture.join(old))?;
            append.write_all(b" plus changed bytes")?;
            append.sync_all()?;
            drop(append);
            collect(watch, observed, evidence, &[(Kind::Modified, name(old))])?;
            watch.arm()?;
            fs::rename(fixture.join(old), fixture.join(new))?;
            collect(
                watch,
                observed,
                evidence,
                &[
                    (Kind::RenameOldName, name(old)),
                    (Kind::RenameNewName, name(new)),
                ],
            )?;
            watch.arm()?;
            fs::remove_file(fixture.join(new))?;
            collect(watch, observed, evidence, &[(Kind::Removed, name(new))])?;
            check(
                observed.kinds & 62 == 62,
                "all five actual native actions observed",
            )?;
            check(observed.wrong_ack_checked, "wrong ACK controls executed")?;
        }
        "quiet" => {
            watch.arm()?;
            loop {
                match watch.poll(Duration::from_millis(10))? {
                    Poll::Pending => {}
                    Poll::DeadlineReached => break,
                    Poll::Frame { .. } => {
                        return Err(Failure::Check("unexpected quiet-directory event"));
                    }
                }
            }
            check(
                watch.snapshot().read_in_flight,
                "deadline did not fake IO completion",
            )?;
            check(
                watch.arm() == Err(Fault::WrongState),
                "deadline did not reopen pending operation",
            )?;
        }
        "stop" | "actual-close-refusal" => {
            watch.arm()?;
            check(
                watch.snapshot().read_in_flight,
                "stop has original pending read",
            )?;
        }
        "unacknowledged" => {
            watch.arm()?;
            fs::write(fixture.join("pending.txt"), b"not acknowledged")?;
            loop {
                match watch.poll(Duration::from_millis(50))? {
                    Poll::Pending => {}
                    Poll::DeadlineReached => {
                        return Err(Failure::Check("pending-delivery deadline"));
                    }
                    Poll::Frame { sequence, wire } => {
                        observed.accept(watch, evidence, sequence, wire, false)?;
                        break;
                    }
                }
            }
            check(
                watch.snapshot().delivery_pending,
                "delivery remains pending",
            )?;
            check(
                observed.client.phase() == Phase::Pending,
                "client charged before ACK",
            )?;
        }
        _ => return Err(Failure::Check("unsupported scenario")),
    }
    Ok(())
}

fn main_run() -> Result<(), Failure> {
    let args = std::env::args().collect::<Vec<_>>();
    check(
        args.len() == 4,
        "exact mode, new root and new fixture parent required",
    )?;
    let mode = &args[1];
    check(
        [
            "events",
            "quiet",
            "stop",
            "unacknowledged",
            "reject-file",
            "reject-sync",
            "actual-overflow",
            "actual-close-refusal",
        ]
        .contains(&mode.as_str()),
        "known scenario",
    )?;
    let base = PathBuf::from(&args[2]);
    check(
        base.is_absolute() && !base.exists(),
        "new absolute fixture root only",
    )?;
    let parent = PathBuf::from(&args[3]);
    let cache_root = Path::new(r"D:\build_cache");
    check(
        parent.is_absolute() && parent.starts_with(cache_root) && parent != cache_root,
        "fixture parent confined below the approved cache root",
    )?;
    check(
        base.parent() == Some(parent.as_path()),
        "exact new fixture parent",
    )?;
    check(
        parent.components().all(|part| {
            !matches!(
                part,
                std::path::Component::ParentDir | std::path::Component::CurDir
            )
        }),
        "no parent traversal",
    )?;
    for ancestor in parent.ancestors() {
        use std::os::windows::fs::MetadataExt;
        let metadata = fs::symlink_metadata(ancestor)?;
        check(
            metadata.is_dir() && metadata.file_attributes() & 0x400 == 0,
            "ordinary fixture ancestors only",
        )?;
    }
    check(
        base.file_name().and_then(|s| s.to_str()) == Some(mode.as_str()),
        "mode owns exactly its fixture directory",
    )?;
    fs::create_dir(&base)?;
    let fixture = base.join("fixture");
    let evidence = base.join("evidence");
    fs::create_dir(&fixture)?;
    fs::create_dir(&evidence)?;
    let source = if mode == "reject-file" {
        let file = fixture.join("ordinary.txt");
        fs::write(&file, b"ordinary file")?;
        File::open(file)?
    } else {
        OpenOptions::new()
            .read(true)
            .share_mode(3)
            .custom_flags(if mode == "reject-sync" {
                0x0220_0000
            } else {
                0x4220_0000
            })
            .open(&fixture)?
    };
    if mode == "actual-close-refusal" {
        // Only this new File, on this original thread. Windows really rejects
        // its later CloseHandle call; no synthetic return value is substituted.
        let raw = source.as_raw_handle();
        let mut flags = 0;
        let set = unsafe {
            SetHandleInformation(
                raw,
                HANDLE_FLAG_PROTECT_FROM_CLOSE,
                HANDLE_FLAG_PROTECT_FROM_CLOSE,
            )
        };
        let queried = unsafe { GetHandleInformation(raw, &mut flags) };
        if set == 0 || queried == 0 || flags & HANDLE_FLAG_PROTECT_FROM_CLOSE == 0 {
            retain_unknown("original close-refusal fixture flag establishment");
        }
        println!("NATIVE_WATCH_ORIGINAL_CLOSE_PROTECTION_CONFIRMED");
    }
    let duration = if mode == "quiet" {
        Duration::from_millis(80)
    } else if mode == "actual-overflow" {
        Duration::from_secs(45)
    } else {
        Duration::from_secs(5)
    };
    let (mut watch, start_error) = match NativeWatch::open(
        source,
        Window {
            epoch: [0x42; 32],
            max_frames: 32,
            duration,
        },
    ) {
        Ok(owner) => (owner, None),
        Err(failure) => (failure.owner, Some(failure.reason)),
    };
    let mut observed = Observation::new()?;
    let business = if mode == "reject-file" {
        check(
            start_error == Some(Fault::Identity),
            "ordinary file refused before mode query or event creation",
        )
    } else if mode == "reject-sync" {
        check(
            start_error == Some(Fault::SynchronousSource),
            "synchronous directory refused before event/read",
        )
    } else if let Some(reason) = start_error {
        Err(Failure::Native(reason))
    } else {
        scenario(mode, &fixture, &evidence, &mut watch, &mut observed)
    };
    if watch.snapshot().unknown {
        retain_unknown("original watch outcome");
    }
    if mode == "actual-close-refusal" {
        business?;
        let refused = watch.retire(Duration::from_secs(5));
        let snapshot = watch.snapshot();
        println!("NATIVE_WATCH_CLOSE_REFUSAL {snapshot:?}");
        check(
            matches!(
                refused,
                Err(Fault::Native {
                    call: morrow_fs_watch_native_v1::windows::Call::CloseDirectory,
                    code: 6,
                })
            ),
            "actual original directory close refused with Windows error 6",
        )?;
        check(
            snapshot.unknown
                && !snapshot.retired
                && !snapshot.read_in_flight
                && snapshot.owned_handles == 2
                && snapshot.close_calls == 1
                && snapshot.confirmed_closed == 0
                && snapshot.read_calls == 1
                && snapshot.completions == 1
                && snapshot.cancel_called,
            "cancellation completed but original native ownership is unresolved",
        )?;
        // Intentional negative fixture: retain both exact handles and storage
        // until this isolated process exits. Never remove the flag or retry.
        std::mem::forget(watch);
        println!(
            "NATIVE_WATCH_EXPECTED_CLOSE_REFUSAL|original_close_calls=1|confirmed_closed=0|native_retirement=false|retry=0|retained_until_process_exit=true"
        );
        return Ok(());
    }
    let closed = match watch.retire(Duration::from_secs(5)) {
        Ok(value) => value,
        Err(_) => retain_unknown("original native retirement"),
    };
    let snapshot = closed.snapshot();
    if watch.retire(Duration::from_secs(5)).is_ok() || watch.snapshot() != snapshot {
        retain_unknown("duplicate native retirement changed original state");
    }
    println!("NATIVE_WATCH_RETIREMENT {snapshot:?}");
    // Report the original operation failure before secondary success assertions.
    business?;
    check(
        snapshot.retired
            && !snapshot.unknown
            && !snapshot.read_in_flight
            && !snapshot.mode_query_pending,
        "all native references retired",
    )?;
    check(
        snapshot.owned_handles == snapshot.close_calls
            && snapshot.close_calls == snapshot.confirmed_closed,
        "exact original native closes",
    )?;
    if mode == "reject-file" {
        check(
            snapshot.owned_handles == 1
                && snapshot.read_calls == 0
                && snapshot.first_error == Some(Fault::Identity),
            "initial refusal preserved",
        )?;
    } else if mode == "reject-sync" {
        check(
            snapshot.owned_handles == 1
                && snapshot.read_calls == 0
                && snapshot.mode_query_calls == 1
                && !snapshot.asynchronous_verified
                && snapshot.first_error == Some(Fault::SynchronousSource),
            "actual synchronous mode refusal",
        )?;
    } else {
        check(
            snapshot.owned_handles == 2
                && snapshot.identity_checked
                && snapshot.asynchronous_verified
                && snapshot.mode_query_calls == 1
                && snapshot.first_error.is_none(),
            "two actual same-object native handles",
        )?;
    }
    if mode == "quiet" || mode == "stop" {
        check(
            snapshot.cancel_called
                && snapshot.read_calls == 1
                && snapshot.completions == 1
                && observed.frames == 0,
            "cancel separately completed",
        )?;
    }
    if mode == "unacknowledged" {
        let usage = observed.client.usage();
        observed.client.transport_lost();
        check(
            snapshot.delivery_pending
                && observed.client.phase() == Phase::Unknown
                && observed.client.usage() == usage,
            "native close does not erase delivery Unknown",
        )?;
    }
    println!(
        "NATIVE_WATCH_CLOSED|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}",
        mode,
        snapshot.owned_handles,
        snapshot.close_calls,
        snapshot.confirmed_closed,
        snapshot.read_calls,
        snapshot.completions,
        observed.frames,
        u8::from(snapshot.cancel_called),
        u8::from(snapshot.delivery_pending),
        observed.kinds,
        snapshot.discarded_completion_bytes,
        u8::from(snapshot.original_directory_consumed && snapshot.confirmed_closed >= 1)
    );
    Ok(())
}

fn main() {
    std::panic::set_hook(Box::new(|_| {
        retain_unknown("panic before original owner unwinding")
    }));
    if let Err(error) = main_run() {
        eprintln!("NATIVE_WATCH_PROBE_FAILED {error}");
        std::process::exit(1);
    }
}
