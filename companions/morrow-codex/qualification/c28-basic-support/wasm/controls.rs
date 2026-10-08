//! Direct original Workbench command lane; no synthetic owner or authority.
#![forbid(unsafe_code)]
use morrow_agent_process_control_v1::{self as process, Action, EventKind, OutputStream, ReadQuery, ReplyBody, host::Handle};
use morrow_core::task::{Invocation, Transform};
use morrow_workbench_host::{Workbench, agent_tasks::{AgentCommand, AgentReply}, io_tasks::TaskKey};
use std::time::{Duration, Instant};
use zeroize::Zeroizing;

/// Waits once on the original command handle. Error never authorizes resubmission.
pub fn run_task(workbench: &Workbench, key: TaskKey, task: &Invocation, calls: u64, deadline: Instant) -> Result<Vec<u8>, String> {
    let mut command = workbench.submit_agent(key, AgentCommand::RunFrame(Zeroizing::new(task.bytes().to_vec())))
        .map_err(|e| format!("submit: {e:?}"))?;
    loop {
        match command.try_read().map_err(|e| format!("delivery Unknown: {e:?}"))? {
            Some(reply) => {
                let AgentReply::Frame(run) = &reply else { return Err("unexpected command reply; Unknown".into()) };
                if run.report.outcome != Ok(0) || u64::from(run.report.host_calls) != calls {
                    return Err(format!("guest outcome {:?}, host calls {}; no retry", run.report.outcome, run.report.host_calls));
                }
                let completion = run.completion.as_ref().ok_or("missing completion; Unknown")?;
                let output = task.verify_output(completion).map_err(|e| format!("task correlation: {e:?}"))?;
                if output.bytes.len() > process::MAX_FRAME_BYTES { return Err("output limit; Unknown".into()) }
                return Ok(output.bytes.to_vec());
            }
            None if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(5)),
            None => {
                command.cancel();
                return Err("delivery deadline; Unknown; original owner cleanup required".into());
            }
        }
    }
}

#[derive(Debug, Default)]
pub struct ControlEvidence {
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
    pub after_seq: u64,
    pub exit_code: Option<i32>,
    pub saw_exit: bool,
    pub saw_closed: bool,
    pub guest_commands: u32,
    pub host_calls: u64,
    pub closed_write_rejected: bool,
}
impl ControlEvidence {
    fn accept(&mut self, page: process::OutputPage) -> Result<(), String> {
        if page.gap || page.failure.is_some() { return Err(format!("incomplete observation: {page:?}")) }
        for event in page.events {
            if event.seq != self.after_seq + 1 || self.saw_closed { return Err("event sequence incomplete".into()) }
            self.after_seq = event.seq;
            match event.kind {
                EventKind::Output { stream, chunk } => match stream {
                    OutputStream::Stdout | OutputStream::Pty => self.stdout.extend(chunk),
                    OutputStream::Stderr => self.stderr.extend(chunk),
                },
                EventKind::Exited { exit_code, .. } => {
                    if self.saw_exit { return Err("duplicate exit".into()) }
                    self.saw_exit = true;
                    self.exit_code = Some(exit_code);
                }
                EventKind::Closed => {
                    if !self.saw_exit { return Err("EOF without real exit event".into()) }
                    self.saw_closed = true;
                }
            }
        }
        if self.stdout.len() + self.stderr.len() > 65536 || page.next_seq != self.after_seq
            || page.exited != self.saw_exit || page.closed != self.saw_closed || page.exit_code != self.exit_code {
            return Err("bounded complete observation mismatch".into());
        }
        Ok(())
    }
}

pub struct ControlSession {
    key: TaskKey,
    handle: Handle,
    prefix: String,
    sequence: u32,
    deadline: Instant,
    controls_unknown: bool,
    max_commands: u32,
    pub evidence: ControlEvidence,
}
impl ControlSession {
    pub fn new(key: TaskKey, handle: Handle, nonce: &str) -> Result<Self, String> {
        Self::with_budget(key, handle, nonce, 15)
    }
    /// The formal sealed proposal/review/Claim/start chain already spent four
    /// of the original sixteen worker commands. Never expand that SDK ceiling.
    pub fn with_budget(key: TaskKey, handle: Handle, nonce: &str, max_commands: u32) -> Result<Self, String> {
        if !crate::witness::valid_nonce(nonce) { return Err("invalid witness nonce".into()) }
        if max_commands == 0 || max_commands > 15 { return Err("remaining worker command budget".into()) }
        Ok(Self { key, handle, prefix: format!("vm-{nonce}"), sequence: 0, deadline: Instant::now() + Duration::from_secs(45), controls_unknown: false, max_commands, evidence: ControlEvidence::default() })
    }
    pub fn controls_unknown(&self) -> bool { self.controls_unknown }
    pub fn call(&mut self, workbench: &Workbench, action: Action) -> Result<ReplyBody, String> {
        if action.is_mutation() && self.controls_unknown { return Err("Unknown controls latched; no replay".into()) }
        if self.sequence >= self.max_commands || Instant::now() >= self.deadline { return Err("original worker command/deadline budget; cleanup required".into()) }
        self.sequence += 1;
        let id = format!("{}-{}", self.prefix, self.sequence);
        let input = process::Request::new(&id, self.handle.nonce, self.handle.generation, action.clone()).map_err(|e| e.to_string())?;
        let calls = if matches!(action, Action::Discover) { 1 } else { 2 };
        let actual = process::Request::new(format!("{id}-{calls}"), self.handle.nonce, self.handle.generation, action.clone()).map_err(|e| e.to_string())?;
        let task = Invocation::new_transform(&id, Transform { handler: "codex.process.control".into(), input_type: "codex.process.request.v1".into(), output_type: "codex.process.reply.v1".into(), input: input.encode().map_err(|e| e.to_string())? }).map_err(|e| format!("task: {e:?}"))?;
        let mutation = action.is_mutation();
        if mutation { self.controls_unknown = true }
        self.evidence.guest_commands += 1;
        let raw = run_task(workbench, self.key, &task, calls, self.deadline)?;
        let reply = process::Reply::decode_for(&actual, &raw).map_err(|e| format!("canonical correlation Unknown: {e:?}"))?;
        self.evidence.host_calls += calls;
        match &reply.body {
            ReplyBody::Accepted | ReplyBody::Rejected(_) if reply.body != ReplyBody::Rejected(process::Error::Unknown) => {
                if mutation { self.controls_unknown = false }
            }
            ReplyBody::Rejected(process::Error::Unknown) => self.controls_unknown = true,
            _ if mutation => return Err("unexpected mutation result; Unknown latched".into()),
            _ => {}
        }
        Ok(reply.body)
    }
    fn accepted(&mut self, workbench: &Workbench, action: Action) -> Result<(), String> {
        match self.call(workbench, action)? { ReplyBody::Accepted => Ok(()), body => Err(format!("control not accepted: {body:?}; no replay")) }
    }
    fn collect(&mut self, workbench: &Workbench, marker: Option<&str>) -> Result<(), String> {
        loop {
            let q = ReadQuery { after_seq: self.evidence.after_seq, max_bytes: 32768, max_events: 16, wait_ms: 1000 };
            match self.call(workbench, Action::Events(q))? {
                ReplyBody::Page(page) => self.evidence.accept(page)?,
                body => return Err(format!("observation unavailable: {body:?}")),
            }
            if let Some(text) = marker {
                if String::from_utf8_lossy(&self.evidence.stdout).lines().any(|line| line.contains(text)) { return Ok(()) }
            } else if self.evidence.saw_closed {
                if self.evidence.exit_code != Some(0) { return Err("child nonzero exit".into()) }
                return Ok(());
            }
            if self.evidence.saw_closed { return Err("child exited before witness marker".into()) }
        }
    }
}

/// Failure requires trusted cleanup of the same owner; never call again to retry effects.
pub fn run_pipe_eof(workbench: &Workbench, key: TaskKey, handle: Handle, nonce: &str) -> Result<ControlEvidence, String> {
    let mut control = ControlSession::with_budget(key, handle, nonce, 12)?;
    let ReplyBody::Capabilities(caps) = control.call(workbench, Action::Discover)? else { return Err("missing capabilities".into()) };
    if !caps.write || !caps.close_input || !caps.events { return Err("actual provider lacks pipe controls".into()) }
    let mut input = format!("VM-INPUT {nonce}\n").into_bytes();
    input.extend_from_slice(b"\0\xffbounded-tail");
    control.accepted(workbench, Action::Write(input.clone()))?;
    control.accepted(workbench, Action::CloseInput)?;
    if control.call(workbench, Action::Write(b"must-not-reach-child".to_vec()))? != ReplyBody::Rejected(process::Error::Closed) {
        return Err("closed write did not return canonical correlated Closed".into());
    }
    control.evidence.closed_write_rejected = true;
    control.collect(workbench, None)?;
    let marker = format!("WITNESS-EOF {nonce} {} {}", input.len(), crate::witness::hex(&process::hash(&input)));
    if !String::from_utf8_lossy(&control.evidence.stdout).lines().any(|line| line.trim_end_matches('\r') == marker)
        || !String::from_utf8_lossy(&control.evidence.stderr).contains(&format!("WITNESS-STDERR {nonce}")) {
        return Err("actual child EOF byte count/hash/stderr mismatch".into());
    }
    Ok(control.evidence)
}

pub fn run_resize(workbench: &Workbench, key: TaskKey, handle: Handle, nonce: &str) -> Result<ControlEvidence, String> {
    let mut control = ControlSession::with_budget(key, handle, nonce, 12)?;
    let ReplyBody::Capabilities(caps) = control.call(workbench, Action::Discover)? else { return Err("missing capabilities".into()) };
    if !caps.write || !caps.resize_pty || !caps.events { return Err("actual provider lacks PTY controls".into()) }
    for (name, rows, cols) in [("size-1", 25, 91), ("size-2", 33, 117)] {
        control.accepted(workbench, Action::Resize { rows, cols })?;
        control.accepted(workbench, Action::Write(format!("{name}\n").into_bytes()))?;
        let marker = format!("WITNESS-SIZE {nonce} {name} buffer={rows}x{cols} viewport=");
        control.collect(workbench, Some(&marker))?;
    }
    control.accepted(workbench, Action::Write(b"exit\n".to_vec()))?;
    control.collect(workbench, None)?;
    Ok(control.evidence)
}

/// Borrow the original context until the original owner attaches its Worker.
/// A rejection keeps `prepared` unchanged; the caller explicitly disposes it
/// through that same Workbench, retaining any pending cleanup as original debt.
/// This wrapper never retries, disposes, or fabricates a second owner.
pub fn start_session_frame(
    options: morrow_workbench_host::agent_tasks::AgentStart,
    owner: &mut Workbench,
    prepared: &mut Option<morrow_workbench_host::agent_tasks::AgentContext>,
) -> Result<TaskKey, String> {
    owner.start_agent(options, prepared).map_err(|error| format!("agent start rejected; original context or owner debt retained: {error}"))
}
