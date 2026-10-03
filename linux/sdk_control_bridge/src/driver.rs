//! Native original-object/lease driver is separate from actual SQL/Wasmi work.
use crate::{
    Result,
    worker::{Event, Job, Worker},
};
use morrow_linux_supervisor_foundation::{
    ControllerLoss, FrameKind, FrameRead, take_supervisor_fixture_transport,
};
use morrow_workbench_host::product_gate::ProductGate;
use std::{
    path::Path,
    sync::{atomic::Ordering, mpsc::TrySendError},
    time::{Duration, Instant},
};
pub fn supervisor(repo: &Path, root: &Path, language: &str) -> Result<()> {
    let (mut transport, mut watch) = unsafe { take_supervisor_fixture_transport() }?;
    println!(
        "bridge-S pid={} parent={} original-C={};topology=G-owned-C-S-siblings;protected-owner=false",
        std::process::id(),
        unsafe { libc::getppid() },
        watch.diagnostic_pid()
    );
    let gate = ProductGate::default();
    let mut worker = Worker::start(
        repo.to_path_buf(),
        root.to_path_buf(),
        language.to_string(),
        gate.clone(),
    )?;
    let result = (|| -> Result<()> {
        let started = Instant::now();
        let mut ready = false;
        let mut admitted = false;
        let mut closed = false;
        let mut stop_sent = false;
        let mut proof = None;
        while started.elapsed() < Duration::from_secs(7) {
            let now = started.elapsed();
            let live = watch.poll();
            if !live && !closed {
                gate.revoke();
                closed = true;
                transport.retire();
                worker.released.store(true, Ordering::Release);
                println!(
                    "native-original-loss={:?};gate-closed=true;stop-original-without-SQL-worker-lock=true",
                    watch.loss().map(|l| l.reason)
                );
            }
            if !closed {
                match transport.poll_read(now)? {
                    FrameRead::Frame(frame) => match frame.kind() {
                        FrameKind::Heartbeat => {
                            watch.heartbeat(&frame)?;
                            if ready && !admitted {
                                match worker.sender.try_send(Job::Start) {
                                    Ok(()) => admitted = true,
                                    Err(TrySendError::Full(_)) => (),
                                    Err(_) => return Err("worker lost before admission".into()),
                                }
                            }
                        }
                        FrameKind::Data
                            if ready && admitted && watch.poll() && watch.admission_open() =>
                        {
                            if frame.payload().len() < 2 {
                                return Err("empty SDK fixture call".into());
                            }
                            let job = Job::Call {
                                tag: frame.payload()[0],
                                bytes: frame.payload()[1..].to_vec(),
                            };
                            if worker.sender.try_send(job).is_err() {
                                watch.revoke(ControllerLoss::Backpressure);
                            }
                        }
                        FrameKind::GracefulClose => watch.revoke(ControllerLoss::TransportEof),
                        _ => watch.revoke(ControllerLoss::Protocol),
                    },
                    FrameRead::Eof => watch.revoke(ControllerLoss::TransportEof),
                    FrameRead::Pending => (),
                }
            }
            for _ in 0..4 {
                match worker.events.try_recv() {
                    Ok(Event::Ready {
                        directory,
                        cleanup: _,
                    }) => {
                        if ready {
                            return Err("duplicate SDK worker ready".into());
                        }
                        ready = true;
                        if !closed {
                            transport.queue(FrameKind::Status, &directory, started.elapsed())?;
                        }
                    }
                    Ok(Event::Reply(bytes)) => {
                        if !closed && watch.poll() && watch.admission_open() {
                            transport.queue(FrameKind::Reply, &bytes, started.elapsed())?;
                        }
                    }
                    Ok(Event::Held) => {
                        if !closed {
                            transport.queue(
                                FrameKind::Status,
                                b"worker-held",
                                started.elapsed(),
                            )?;
                        }
                    }
                    Ok(Event::Failure(error)) => {
                        if !closed {
                            transport.queue(
                                FrameKind::Diagnostic,
                                error.as_bytes(),
                                started.elapsed(),
                            )?;
                        } else {
                            println!("pending-business-outcome=Unknown;no-auto-replay=true");
                        }
                    }
                    Ok(Event::SourceFailure(error)) => {
                        eprintln!("ACTUAL-PRODUCER-ERROR: {error}");
                        if !closed {
                            transport.queue(
                                FrameKind::Diagnostic,
                                error.as_bytes(),
                                started.elapsed(),
                            )?;
                        }
                    }
                    Ok(Event::SourceTerminal(error)) => {
                        // A real source-local Closed/Denied/Expired is not a native
                        // protocol failure and never becomes business success.
                        println!("actual-source-terminal={error}");
                    }
                    Ok(Event::ChannelClosed(snapshot)) => {
                        if closed
                            || !watch.poll()
                            || !watch.admission_open()
                            || gate.closed()
                            || worker.finished()
                            || transport.io_retired()
                        {
                            return Err(
                                "channel Joined witness lost live original control/worker".into()
                            );
                        }
                        println!(
                            "actual-sdk-channel-close-broker-Joined=true;native-watch-live=true;product-gate-open=true;actual-worker-live=true;native-control-io-retired=false;producer-outcome={:?};last-acked={};accepted-sequence={}",
                            snapshot.producer_outcome,
                            snapshot.last_acked,
                            snapshot.accepted_sequence
                        );
                    }
                    Ok(Event::Proof(value)) => proof = Some(value),
                    Err(std::sync::mpsc::TryRecvError::Empty) => break,
                    Err(std::sync::mpsc::TryRecvError::Disconnected) => break,
                }
            }
            if !closed {
                if transport.poll_write(started.elapsed()).is_err() {
                    watch.revoke(ControllerLoss::Backpressure);
                }
            }
            if closed && !stop_sent {
                stop_sent = worker.sender.try_send(Job::Stop).is_ok();
            }
            if worker.finished() {
                // The worker may send its final receipt after the last Empty
                // observation. The helper drains that now-stable bounded queue
                // before consuming the real join handle.
                let final_proof = worker.stop_and_join_bounded()?;
                if proof.is_none() {
                    proof = final_proof;
                }
                if !closed || proof.is_none() {
                    return Err("actual worker exited without cleanup receipt".into());
                }
                println!(
                    "{};actual-original-worker-joined=true;native-control-io-retired={};tree-empty=unproved;Released=false",
                    proof.unwrap(),
                    transport.io_retired()
                );
                return Ok(());
            }
            std::thread::sleep(Duration::from_millis(1));
        }
        Err("bounded native SDK driver deadline; cleanup unconfirmed".into())
    })();
    // Every Result exit, including authenticated malformed/read/queue errors,
    // first closes exact original admission without waiting for SQL/worker locks.
    gate.revoke();
    transport.retire();
    worker.released.store(true, Ordering::Release);
    let cleanup = worker.stop_and_join_bounded();
    match result {
        Err(primary) => {
            match cleanup {
                Ok(proof) => {
                    if let Some(proof) = proof {
                        println!("error-cleanup-proof={proof}");
                    }
                    println!(
                        "native-error-gate-closed={};native-control-io-retired={};error-original-worker-joined=true;primary-error-retained=true;failed-call-is-not-success=true",
                        gate.closed(),
                        transport.io_retired()
                    );
                }
                Err(secondary) => {
                    eprintln!("ERROR-CLEANUP-UNCONFIRMED: {secondary};primary-error-retained=true")
                }
            }
            Err(primary)
        }
        Ok(()) => {
            cleanup?;
            Ok(())
        }
    }
}
