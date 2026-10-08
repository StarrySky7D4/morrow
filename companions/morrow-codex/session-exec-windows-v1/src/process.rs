use codex_exec_server::{
    ExecOutputStream, ExecProcess, ExecProcessEvent, ProcessControlCapabilities, ProcessControlOutcome, ProcessSignal, WriteStatus,
};
use codex_sandboxing::SandboxType;
use morrow_agent_process_control_v1::{
    Capabilities, Error, EventKind, OutputPage, OutputStream, ProcessEvent, ReadQuery, Result,
    host::{EffectOutcome, ProcessProvider},
};
use morrow_agent_session_exec_v1_r2::{ExecutionFacts, MAX_OUTPUT_BYTES, safe_exec::ToolIdentity};
use sha2::{Digest, Sha256};
use std::{
    collections::VecDeque,
    sync::{Arc, Condvar, Mutex},
    time::Duration,
};
use tokio::runtime::Handle;

#[derive(Default)]
struct Log {
    events: VecDeque<ProcessEvent>,
    bytes: usize,
    floor: u64,
    last: u64,
    exited: bool,
    exit_code: Option<i32>,
    closed: bool,
    failure: Option<String>,
}
pub(crate) struct Observed {
    log: Mutex<Log>,
    changed: Condvar,
}
/// Owns a genuine upstream process. Provider methods never reenter the R2 owner.
/// The host must validate the original executor authority before/after controls.
pub struct WindowsProcessProvider {
    process: Arc<dyn ExecProcess>,
    runtime: Handle,
    observed: Arc<Observed>,
    identity: ToolIdentity,
    writable: bool,
    interruptible: bool,
    controls: ProcessControlCapabilities,
    input_closing: bool,
    sandbox_type: Option<SandboxType>,
    _keepalive: Arc<dyn Send + Sync>,
    slot: Arc<crate::registry::ExecutionSlot>,
}

pub(crate) fn block_on<T: Send>(
    runtime: &Handle,
    future: impl std::future::Future<Output = T> + Send,
) -> T {
    // The port accepts only a live multithread runtime with >= 2 workers. A
    // distinct thread avoids nested block_on; block_in_place releases a Tokio
    // caller's worker so process IO, timers and backend tasks keep advancing.
    let run = || {
        std::thread::scope(|scope| {
            scope
                .spawn(|| runtime.block_on(future))
                .join()
                .expect("native process bridge thread")
        })
    };
    if Handle::try_current()
        .is_ok_and(|h| h.runtime_flavor() == tokio::runtime::RuntimeFlavor::MultiThread)
    {
        tokio::task::block_in_place(run)
    } else {
        run()
    }
}
impl WindowsProcessProvider {
    pub fn observed_identity(&self) -> &ToolIdentity {
        &self.identity
    }
    pub fn sandbox_type(&self) -> Option<SandboxType> {
        self.sandbox_type
    }
    // Keep the independently reviewed execution inputs explicit at this trusted seam.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        process: Arc<dyn ExecProcess>,
        runtime: Handle,
        identity: ToolIdentity,
        writable: bool,
        interruptible: bool,
        controls: ProcessControlCapabilities,
        deadline_ms: u64,
        sandbox_type: Option<SandboxType>,
        on_complete: impl FnOnce(ExecutionFacts) + Send + 'static,
        slot: Arc<crate::registry::ExecutionSlot>,
    ) -> Self {
        let observed = Arc::new(Observed {
            log: Mutex::new(Log {
                floor: 1,
                ..Log::default()
            }),
            changed: Condvar::new(),
        });
        let monitor = observed.clone();
        let original = process.clone();
        let mut receiver = original.subscribe_events();
        slot.provider(true);
        let keepalive: Arc<dyn Send + Sync> = slot.clone();
        let monitor_keepalive = keepalive.clone();
        let monitor_slot = slot.clone();
        let monitor_job = slot.job();
        runtime.spawn(async move {
            let _keepalive = monitor_keepalive; let _monitor_job = monitor_job;
            let mut stdout = Sha256::new(); let mut stderr = Sha256::new();
            let mut stdout_bytes = 0u64; let mut stderr_bytes = 0u64;
            let mut complete_prefix = true; let mut expected = 1;
            let deadline = tokio::time::sleep(Duration::from_millis(deadline_ms));
            tokio::pin!(deadline);
            let mut stopped = false;
            let cleanup = tokio::time::sleep(Duration::from_secs(5));
            tokio::pin!(cleanup);
            loop {
                let event = tokio::select! {
                    result = receiver.recv() => match result {
                        Ok(event) => event,
                        Err(error) => {
                            complete_prefix = false;
                            if let Ok(mut log) = monitor.log.lock() { log.failure = Some(format!("upstream event receiver: {error}")); }
                            monitor.changed.notify_all();
                            if !stopped { let _ = terminate_once(&original, &monitor_slot).await; stopped = true; cleanup.as_mut().reset(tokio::time::Instant::now() + Duration::from_secs(5)); }
                            tokio::time::sleep(Duration::from_millis(20)).await;
                            continue;
                        }
                    },
                    _ = &mut deadline, if !stopped => {
                        let result = terminate_once(&original, &monitor_slot).await;
                        stopped = true;
                        cleanup.as_mut().reset(tokio::time::Instant::now() + Duration::from_secs(5));
                        if result == Some(false) { if let Ok(mut log) = monitor.log.lock() { log.failure = Some("deadline termination outcome unknown".into()); } monitor.changed.notify_all(); }
                        continue;
                    },
                    _ = &mut cleanup, if stopped => {
                        monitor_slot.unknown();
                        if let Ok(mut log) = monitor.log.lock() { log.failure = Some("cleanup did not observe both exit and output closure; native slot retained".into()); }
                        monitor.changed.notify_all(); break;
                    }
                };
                let mapped = match event {
                    ExecProcessEvent::Output(chunk) => {
                        let bytes = chunk.chunk.0;
                        let stream = match chunk.stream {
                            ExecOutputStream::Stdout => OutputStream::Stdout,
                            ExecOutputStream::Stderr => OutputStream::Stderr,
                            ExecOutputStream::Pty => OutputStream::Pty,
                        };
                        let (digest, count) = if stream == OutputStream::Stderr { (&mut stderr, &mut stderr_bytes) } else { (&mut stdout, &mut stdout_bytes) };
                        *count = count.saturating_add(bytes.len() as u64);
                        if *count <= MAX_OUTPUT_BYTES { digest.update(&bytes); } else { complete_prefix = false; }
                        ProcessEvent { seq: chunk.seq, kind: EventKind::Output { stream, chunk: bytes } }
                    }
                    ExecProcessEvent::Exited { seq, exit_code, sandbox_denied } => ProcessEvent { seq, kind: EventKind::Exited { exit_code, sandbox_denied } },
                    ExecProcessEvent::Closed { seq } => ProcessEvent { seq, kind: EventKind::Closed },
                    ExecProcessEvent::Failed(message) => {
                        complete_prefix = false;
                        if let Ok(mut log) = monitor.log.lock() { log.failure = Some(message); }
                        monitor.changed.notify_all();
                        if !stopped { let _ = terminate_once(&original, &monitor_slot).await; stopped = true; cleanup.as_mut().reset(tokio::time::Instant::now() + Duration::from_secs(5)); }
                        continue;
                    }
                };
                if mapped.seq != expected { complete_prefix = false; }
                expected = mapped.seq.saturating_add(1);
                let terminal = {
                    let Ok(mut log) = monitor.log.lock() else { break };
                    if mapped.seq <= log.last { complete_prefix = false; monitor_slot.unknown(); continue; }
                    if mapped.seq > log.last.saturating_add(1) { log.events.clear(); log.bytes = 0; log.floor = mapped.seq; }
                    if log.events.is_empty() { log.floor = mapped.seq; }
                    log.last = mapped.seq;
                    match &mapped.kind {
                        EventKind::Output { chunk, .. } => log.bytes += chunk.len(),
                        EventKind::Exited { exit_code, .. } => { log.exited = true; log.exit_code = Some(*exit_code); },
                        EventKind::Closed => log.closed = true,
                    }
                    log.events.push_back(mapped);
                    while log.bytes > 1024 * 1024 || log.events.len() > 4096 {
                        if let Some(evicted) = log.events.pop_front() {
                            log.floor = evicted.seq.saturating_add(1);
                            if let EventKind::Output { chunk, .. } = evicted.kind { log.bytes -= chunk.len(); }
                        }
                    }
                    if !complete_prefix { log.failure.get_or_insert("incomplete or excessive upstream output; durable facts remain Unknown".into()); }
                    (log.exited && log.closed).then_some(log.exit_code)
                };
                monitor.changed.notify_all();
                if let Some(exit_code) = terminal {
                    monitor_slot.terminal();
                    if complete_prefix {
                        let facts = ExecutionFacts { exit_code, output_closed: true, stdout_sha256: stdout.finalize().into(), stderr_sha256: stderr.finalize().into(), stdout_bytes, stderr_bytes };
                        let completion_job=monitor_slot.job();
                        tokio::task::spawn_blocking(move || { let _completion_job=completion_job; on_complete(facts); });
                    }
                    break;
                }
                if !complete_prefix && !stopped { let _ = terminate_once(&original, &monitor_slot).await; stopped = true; cleanup.as_mut().reset(tokio::time::Instant::now() + Duration::from_secs(5)); }
            }
        });
        Self {
            process,
            runtime,
            observed,
            identity,
            writable,
            interruptible,
            controls,
            input_closing: false,
            sandbox_type,
            _keepalive: keepalive,
            slot,
        }
    }
    fn control_outcome(&self, outcome: Option<ProcessControlOutcome>) -> EffectOutcome {
        match outcome {
            Some(ProcessControlOutcome::Applied) => EffectOutcome::Accepted,
            Some(ProcessControlOutcome::Unsupported) => EffectOutcome::Rejected(Error::Unsupported),
            Some(ProcessControlOutcome::Rejected) => EffectOutcome::Rejected(Error::Conflict),
            None => EffectOutcome::Unknown,
        }
    }
    fn page(&self, query: ReadQuery) -> Result<OutputPage> {
        query.validate()?;
        let mut log = self.observed.log.lock().map_err(|_| Error::Unknown)?;
        if log.last <= query.after_seq && !log.closed && log.failure.is_none() && query.wait_ms > 0
        {
            log = self
                .observed
                .changed
                .wait_timeout(log, Duration::from_millis(query.wait_ms.into()))
                .map_err(|_| Error::Unknown)?
                .0;
        }
        let mut events = Vec::new();
        let mut bytes = 0usize;
        for event in log.events.iter().filter(|e| e.seq > query.after_seq) {
            if events.len() >= query.max_events as usize {
                break;
            }
            let size = if let EventKind::Output { chunk, .. } = &event.kind {
                chunk.len()
            } else {
                0
            };
            if bytes + size > query.max_bytes as usize {
                if events.is_empty() {
                    return Err(Error::Limit);
                }
                break;
            }
            bytes += size;
            events.push(event.clone());
        }
        Ok(OutputPage {
            next_seq: events.last().map_or(query.after_seq, |e| e.seq),
            floor_seq: log.floor,
            gap: query.after_seq.saturating_add(1) < log.floor,
            events,
            exited: log.exited,
            exit_code: log.exit_code,
            closed: log.closed,
            failure: log.failure.clone(),
        })
    }
    fn effect<T: Send + 'static>(
        &self,
        future: impl std::future::Future<
            Output = std::result::Result<T, codex_exec_server::ExecServerError>,
        > + Send
        + 'static,
    ) -> Option<T> {
        let Some(job) = self.slot.job() else {
            self.slot.unknown();
            return None;
        };
        let mut task = self.runtime.spawn(async move {
            let _job = job;
            future.await
        });
        // Dropping a timed-out JoinHandle leaves the actual operation running.
        // Its uncertain receipt freezes mutations in the process-control host.
        match block_on(&self.runtime, async {
            tokio::time::timeout(Duration::from_secs(1), &mut task).await
        }) {
            Ok(Ok(Ok(value))) => Some(value),
            _ => {
                self.slot.unknown();
                None
            }
        }
    }
}
impl ProcessProvider for WindowsProcessProvider {
    fn capabilities(&self) -> Capabilities {
        Capabilities {
            read: true,
            events: true,
            write: self.writable && !self.input_closing,
            close_input: self.writable && self.controls.close_input && !self.input_closing,
            resize_pty: self.controls.resize_pty,
            interrupt: self.interruptible,
            terminate: true,
            ..Capabilities::default()
        }
    }
    fn read(&mut self, query: ReadQuery) -> Result<OutputPage> {
        self.page(query)
    }
    fn events(&mut self, query: ReadQuery) -> Result<OutputPage> {
        self.page(query)
    }
    fn write(&mut self, chunk: &[u8]) -> EffectOutcome {
        if self.input_closing {
            return EffectOutcome::Rejected(Error::Closed);
        }
        if !self.writable {
            return EffectOutcome::Rejected(Error::Unsupported);
        }
        if chunk.is_empty() || chunk.len() > 32768 {
            return EffectOutcome::Rejected(Error::Limit);
        }
        let process = self.process.clone();
        let chunk = chunk.to_vec();
        match self.effect(async move { process.write(chunk).await }) {
            Some(response) if response.status == WriteStatus::Accepted => EffectOutcome::Accepted,
            // Upstream does not promise zero effects for every negative receipt.
            _ => {
                self.slot.unknown();
                EffectOutcome::Unknown
            }
        }
    }
    fn close_input(&mut self) -> EffectOutcome {
        if self.input_closing {
            return EffectOutcome::Rejected(Error::Closed);
        }
        if !self.writable || !self.controls.close_input {
            return EffectOutcome::Rejected(Error::Unsupported);
        }
        // Never reopen admission or resend a possibly-applied close after a
        // timeout, failed ACK or caller cancellation. The original host also
        // fences Unknown; native jobs remain charged until actual completion.
        self.input_closing = true;
        let process = self.process.clone();
        self.control_outcome(self.effect(async move { process.close_input_checked().await }))
    }
    fn resize(&mut self, rows: u16, cols: u16) -> EffectOutcome {
        if !(1..=4096).contains(&rows) || !(1..=4096).contains(&cols) {
            return EffectOutcome::Rejected(Error::Invalid);
        }
        if !self.controls.resize_pty {
            return EffectOutcome::Rejected(Error::Unsupported);
        }
        let process = self.process.clone();
        self.control_outcome(self.effect(async move { process.resize_checked(rows, cols).await }))
    }
    fn interrupt(&mut self) -> EffectOutcome {
        if !self.interruptible {
            return EffectOutcome::Rejected(Error::Unsupported);
        }
        let process = self.process.clone();
        match self.effect(async move { process.signal(ProcessSignal::Interrupt).await }) {
            Some(()) => EffectOutcome::Accepted,
            None => EffectOutcome::Unknown,
        }
    }
    fn terminate(&mut self) -> EffectOutcome {
        if !self.slot.claim_cleanup() {
            return EffectOutcome::Unknown;
        }
        let process = self.process.clone();
        match self.effect(async move { process.terminate().await }) {
            Some(()) => EffectOutcome::Accepted,
            None => EffectOutcome::Unknown,
        }
    }
}
impl Drop for WindowsProcessProvider {
    fn drop(&mut self) {
        let process = self.process.clone();
        let slot = self.slot.clone();
        let job = slot.job();
        slot.provider(false);
        self.runtime.spawn(async move {
            let _job = job;
            cleanup_actual(process, &slot).await;
        });
    }
}
pub(crate) async fn cleanup_actual(
    process: Arc<dyn ExecProcess>,
    slot: &Arc<crate::registry::ExecutionSlot>,
) {
    let _ = terminate_once(&process, slot).await;
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    while tokio::time::Instant::now() < deadline {
        if let Ok(Ok(read)) = tokio::time::timeout(
            Duration::from_millis(200),
            process.read(None, Some(1), Some(100)),
        )
        .await
            && read.exited
            && read.closed
        {
            slot.terminal();
            return;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    // Ownership and its charge remain in the registry until genuine exit+EOF.
    slot.unknown();
}

/// Every trusted and guest termination path consumes the same per-slot marker.
/// An uncertain native effect is never retried; subsequent cleanup may read.
async fn terminate_once(
    process: &Arc<dyn ExecProcess>,
    slot: &Arc<crate::registry::ExecutionSlot>,
) -> Option<bool> {
    if !slot.claim_cleanup() {
        return None;
    }
    let result = tokio::time::timeout(Duration::from_secs(1), process.terminate()).await;
    let accepted = matches!(result, Ok(Ok(())));
    if !accepted {
        slot.unknown();
    }
    Some(accepted)
}
