//! Read-only observation of the original sealed process guest.
//! A page supplied in a unit test is never accepted as production facts.
use anyhow::{Result, ensure, bail};
use morrow_agent_process_control_v1::{
    Action, EventKind, OutputPage, OutputStream, ReadQuery, ReplyBody, host::Handle,
};
use morrow_workbench_host::{Workbench, io_tasks::TaskKey};
use std::{fs::{File, OpenOptions}, io::{Read, Write}, path::{Path, PathBuf},
    os::windows::fs::OpenOptionsExt};

#[derive(Default)]
pub struct TerminalEvidence {
    pub stdout: Vec<u8>, pub stderr: Vec<u8>, pub after_seq: u64,
    pub exit_code: Option<i32>, pub saw_exit: bool, pub saw_closed: bool,
    pub guest_commands: u32, pub host_calls: u64,
}
impl TerminalEvidence {
    fn accept(&mut self, page: OutputPage) -> Result<()> {
        page.validate().map_err(anyhow::Error::msg)?;
        ensure!(!page.gap && page.failure.is_none(), "incomplete basic observation");
        for event in page.events {
            ensure!(event.seq == self.after_seq + 1 && !self.saw_closed, "basic event sequence");
            self.after_seq = event.seq;
            match event.kind {
                EventKind::Output { stream, chunk } => {
                    ensure!(self.stdout.len() + self.stderr.len() + chunk.len() <= 65536, "basic output bound");
                    match stream {
                        OutputStream::Stdout => self.stdout.extend(chunk),
                        OutputStream::Stderr => self.stderr.extend(chunk),
                        OutputStream::Pty => bail!("basic is pipe-only"),
                    }
                }
                EventKind::Exited { exit_code, sandbox_denied } => {
                    ensure!(!self.saw_exit && sandbox_denied != Some(true), "duplicate/denied basic exit");
                    self.saw_exit = true; self.exit_code = Some(exit_code);
                }
                EventKind::Closed => {
                    ensure!(self.saw_exit, "EOF cannot substitute for actual exit");
                    self.saw_closed = true;
                }
            }
        }
        ensure!(page.next_seq == self.after_seq && page.exited == self.saw_exit
            && page.closed == self.saw_closed && page.exit_code == self.exit_code,
            "basic page terminal summary mismatch");
        Ok(())
    }
}

pub struct Fixture {
    pub allowed: PathBuf, pub denied: PathBuf,
    baseline: PathBuf, _directory: File, _baseline: File,
    pub security: crate::basic_security::SecuritySpec,
    _query_pin: codex_windows_sandbox::MatchedRunnerArtifact,
    network: crate::basic_security::NetworkFixture,
}
impl Fixture {
    /// Explicit sealed-basic selection creates these fresh synthetic sentinels.
    /// No overwrite, rollback, adoption or automatic delete after a partial failure.
    pub fn create(lifecycle: &crate::provisioning::GuestLifecycle, nonce: &str,
        query: &crate::preflight::Artifact) -> Result<Self> {
        let root = lifecycle.synthetic_root();
        // Validate and retain the exact guest-system image before fixture effects.
        let query_pin = crate::basic_security::pin_query(query)?;
        let identities = lifecycle.owned_identities().ok_or_else(|| anyhow::anyhow!("actual setup inventory absent"))?;
        let users: Vec<_> = identities.users.iter().filter(|u| u.name == "CodexSandboxOffline").collect();
        ensure!(users.len() == 1 && crate::basic_security::valid_sid(&users[0].sid), "actual unique Offline SID absent");
        let expected_sid = users[0].sid.clone();
        crate::preflight::plain_absolute(root)?;
        crate::preflight::no_reparse_ancestors(root)?;
        let (allowed, denied) = crate::basic_witness::paths(root, nonce)?;
        crate::preflight::no_reparse_ancestors(allowed.parent().unwrap())?;
        ensure!(!allowed.try_exists()? && !root.join("basic-outside").try_exists()?, "basic fixture collision");
        std::fs::create_dir(root.join("basic-outside"))?;
        crate::preflight::no_reparse_ancestors(denied.parent().unwrap())?;
        let directory = OpenOptions::new().read(true).share_mode(1).custom_flags(0x02000000)
            .open(denied.parent().unwrap())?;
        let baseline = denied.parent().unwrap().join("trusted-positive.bin");
        let mut positive = OpenOptions::new().read(true).write(true).create_new(true).share_mode(1)
            .open(&baseline)?;
        positive.write_all(b"C28-TRUSTED-POSITIVE\n")?;
        positive.sync_all()?;
        let network = crate::basic_security::NetworkFixture::create(nonce)?;
        let security = crate::basic_security::SecuritySpec { query: query.clone(), expected_sid, port: network.port };
        Ok(Self { allowed, denied, baseline, _directory: directory, _baseline: positive,
            security, _query_pin: query_pin, network })
    }
    pub fn verify(&self, nonce: &str) -> Result<()> {
        crate::preflight::no_reparse_ancestors(&self.allowed)?;
        let mut file = File::open(&self.allowed)?;
        let expected = crate::basic_witness::file_bytes(nonce);
        ensure!(file.metadata()?.is_file() && file.metadata()?.len() == expected.len() as u64,
            "basic workspace witness type/size");
        let mut raw = Vec::new();
        file.read_to_end(&mut raw)?;
        ensure!(raw == expected && !self.denied.try_exists()?, "basic allow/deny sentinel mismatch");
        ensure!(std::fs::metadata(&self.baseline)?.len() == 21, "trusted positive size changed");
        ensure!(std::fs::read(&self.baseline)? == b"C28-TRUSTED-POSITIVE\n", "trusted positive changed");
        self.network.verify_after_child(nonce)?;
        Ok(())
    }
}

pub fn observe(workbench: &Workbench, key: TaskKey, handle: Handle, nonce: &str, security: &crate::basic_security::SecuritySpec)
    -> Result<TerminalEvidence> {
    // Four sealed proposal/review/claim/start commands leave twelve of the
    // original sixteen. The collector cannot widen that original SDK budget.
    let mut lane = crate::controls::ControlSession::with_budget(key, handle, nonce, 12)
        .map_err(anyhow::Error::msg)?;
    let ReplyBody::Capabilities(caps) = lane.call(workbench, Action::Discover)
        .map_err(anyhow::Error::msg)? else { bail!("basic capabilities absent"); };
    ensure!(caps.read && caps.events && !caps.write && !caps.close_input && !caps.resize_pty
        && !caps.terminate && !caps.interrupt, "basic must retain read/events-only process ceiling");
    let mut result = TerminalEvidence::default();
    loop {
        let query = ReadQuery { after_seq: result.after_seq, max_bytes: 32768,
            max_events: 16, wait_ms: 1000 };
        match lane.call(workbench, Action::Events(query)).map_err(anyhow::Error::msg)? {
            ReplyBody::Page(page) => result.accept(page)?,
            _ => bail!("basic observation unavailable; Unknown, no replay"),
        }
        if result.saw_closed { break; }
    }
    ensure!(result.saw_exit && result.exit_code == Some(0)
        && result.stdout == crate::basic_witness::expected_security_stdout(nonce, security)
        && result.stderr == crate::basic_witness::expected_stderr(nonce),
        "basic actual stdout/stderr/exit mismatch");
    result.guest_commands = lane.evidence.guest_commands;
    result.host_calls = lane.evidence.host_calls;
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use morrow_agent_process_control_v1::ProcessEvent;
    fn page(events: Vec<ProcessEvent>, exit: Option<i32>, closed: bool) -> OutputPage {
        OutputPage { next_seq: events.last().map_or(0, |e| e.seq), floor_seq: 1,
            events, gap: false, exited: exit.is_some(), exit_code: exit, closed, failure: None }
    }
    #[test]
    fn terminal_observation_requires_both_actual_exit_and_eof() {
        let mut evidence = TerminalEvidence::default();
        evidence.accept(page(vec![ProcessEvent { seq: 1,
            kind: EventKind::Exited { exit_code: 0, sandbox_denied: None } }], Some(0), false)).unwrap();
        assert!(evidence.saw_exit && !evidence.saw_closed);
        evidence.accept(page(vec![ProcessEvent { seq: 2, kind: EventKind::Closed }], Some(0), true)).unwrap();
        assert!(evidence.saw_exit && evidence.saw_closed);
        assert!(TerminalEvidence::default().accept(page(vec![ProcessEvent {
            seq: 1, kind: EventKind::Closed }], None, true)).is_err());
    }
    #[test]
    fn failure_gap_and_pty_cannot_be_basic_terminal_evidence() {
        let mut failure = page(vec![], None, false); failure.failure = Some("lost".into());
        assert!(TerminalEvidence::default().accept(failure).is_err());
        let mut gap = page(vec![], None, false); gap.gap = true;
        assert!(TerminalEvidence::default().accept(gap).is_err());
        assert!(TerminalEvidence::default().accept(page(vec![ProcessEvent { seq: 1,
            kind: EventKind::Output { stream: OutputStream::Pty, chunk: vec![1] } }], None, false)).is_err());
    }
}
