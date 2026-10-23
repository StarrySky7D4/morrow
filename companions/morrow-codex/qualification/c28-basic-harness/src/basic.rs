//! Read-only observation of the original sealed process guest.
//! A page supplied in a unit test is never accepted as production facts.
use anyhow::{Result, ensure, bail};
use morrow_agent_process_control_v1::{
    Action, EventKind, OutputPage, OutputStream, ReadQuery, ReplyBody, host::Handle,
};
use morrow_workbench_host::{Workbench, io_tasks::TaskKey};
use sha2::{Digest, Sha256};
use std::{fs::{File, OpenOptions}, io::{Read, Write}, path::{Path, PathBuf},
    os::windows::fs::OpenOptionsExt};

#[derive(Default)]
pub struct TerminalEvidence {
    pub stdout: Vec<u8>, pub stderr: Vec<u8>, pub after_seq: u64,
    pub exit_code: Option<i32>, pub saw_exit: bool, pub saw_closed: bool,
    pub guest_commands: u32, pub host_calls: u64,
}
impl TerminalEvidence {
    // Failure pages remain rejected before mutating any accepted evidence.
    // Only finite labels and byte fingerprints leave this diagnostic seam.
    // It never asks the retained owner for another page or supplies a proof.
    fn rejected_page_diagnostic(&self, page: &OutputPage) -> serde_json::Value {
        let mut stderr = self.stderr.clone();
        let mut stdout_bytes = self.stdout.len();
        let mut output_bounded = stderr.len() <= 65536 && stdout_bytes <= 65536;
        for event in &page.events {
            if let EventKind::Output { stream, chunk } = &event.kind {
                match stream {
                    OutputStream::Stdout => stdout_bytes = stdout_bytes.saturating_add(chunk.len()),
                    OutputStream::Stderr if stderr.len().saturating_add(chunk.len()) <= 65536 =>
                        stderr.extend_from_slice(chunk),
                    OutputStream::Stderr | OutputStream::Pty => output_bounded = false,
                }
            }
        }
        output_bounded &= stdout_bytes.saturating_add(stderr.len()) <= 65536;
        let child_predicate = if output_bounded {
            classify_failed_child(&stderr)
        } else { "UNCLASSIFIED_OUTPUT_BOUND" };
        serde_json::json!({
            "schema": "BASIC_REJECTED_PAGE_DIAGNOSTIC_V1", "diagnostic_only": true,
            "gap": page.gap, "provider_failure_present": page.failure.is_some(),
            "provider_failure_sha256": page.failure.as_ref().map(|v|
                crate::preflight::hex(&Sha256::digest(v.as_bytes()))),
            "page_exited": page.exited, "page_exit_code": page.exit_code,
            "page_closed": page.closed, "stdout_bytes": stdout_bytes,
            "captured_stderr_bytes": stderr.len(), "output_bounded": output_bounded,
            "captured_stderr_sha256": crate::preflight::hex(&Sha256::digest(&stderr)),
            "child_predicate": child_predicate, "accepted_terminal_evidence": false,
        })
    }
    fn accept(&mut self, page: OutputPage) -> Result<()> {
        page.validate().map_err(anyhow::Error::msg)?;
        if page.gap || page.failure.is_some() {
            bail!("incomplete basic observation: {}", self.rejected_page_diagnostic(&page));
        }
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

fn classify_failed_child(stderr: &[u8]) -> &'static str {
    let Ok(value) = serde_json::from_slice::<serde_json::Value>(stderr) else {
        return "UNCLASSIFIED";
    };
    if value.get("status").and_then(serde_json::Value::as_str)
        != Some("FAIL_PRESERVED_NO_AUTOMATIC_RETRY") { return "UNCLASSIFIED"; }
    match value.get("message").and_then(serde_json::Value::as_str) {
        Some("restricted child reached owned loopback listener") => "NETWORK_CONNECTED_REJECTED",
        Some("network result is not explicit WSAEACCES; Unknown: connection timed out") =>
            "NETWORK_TIMEOUT_NOT_DENIAL_PROOF",
        Some("artifact ancestor metadata unavailable") => "PRIVATE_ANCESTOR_METADATA_UNAVAILABLE",
        Some("outside write unexpectedly succeeded") => "OUTSIDE_WRITE_SUCCEEDED_REJECTED",
        Some("basic target collision") => "TARGET_COLLISION",
        _ => "UNCLASSIFIED",
    }
}

pub struct Fixture {
    pub allowed: PathBuf, pub denied: PathBuf,
    baseline: PathBuf, _directory: File, _baseline: File,
    _readable_positive: Option<File>,
    pub security: crate::basic_security::SecuritySpec,
    _query_pin: Option<codex_windows_sandbox::MatchedRunnerArtifact>,
    network: crate::basic_security::NetworkFixture,
}
impl Fixture {
    /// Explicit sealed-basic selection creates these fresh synthetic sentinels.
    /// No overwrite, rollback, adoption or automatic delete after a partial failure.
    pub fn create(lifecycle: &crate::provisioning::GuestLifecycle, nonce: &str,
        query: &crate::preflight::Artifact) -> Result<Self> {
        Self::create_with_source(lifecycle,nonce,crate::basic_security::IdentitySource::Whoami(query.clone()))
    }
    pub fn create_native_v8(lifecycle:&crate::provisioning::GuestLifecycle,nonce:&str)->Result<Self> {
        Self::create_with_source(lifecycle,nonce,crate::basic_security::IdentitySource::CurrentPrimaryTokenV8)
    }
    fn create_with_source(lifecycle:&crate::provisioning::GuestLifecycle,nonce:&str,
        source:crate::basic_security::IdentitySource)->Result<Self> {
        let root = lifecycle.synthetic_root();
        // Validate and retain the exact guest-system image before fixture effects.
        // Native v8 queries its own primary token, with no identity child/image.
        let query_pin = source.legacy_query().map(crate::basic_security::pin_query).transpose()?;
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
        let readable_positive = if source.is_native_v8() {
            let path = crate::basic_witness::readable_positive_path(root, nonce)?;
            let mut file = OpenOptions::new().read(true).write(true).create_new(true).share_mode(1)
                .open(path)?;
            file.write_all(b"C28-TRUSTED-POSITIVE\n")?;
            file.sync_all()?;
            crate::basic_witness::verify_readable_positive(root, nonce)?;
            Some(file)
        } else { None };
        let network = crate::basic_security::NetworkFixture::create(nonce)?;
        let security = crate::basic_security::SecuritySpec { source, expected_sid, port: network.port };
        Ok(Self { allowed, denied, baseline, _directory: directory, _baseline: positive,
            security, _readable_positive: readable_positive, _query_pin: query_pin, network })
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
        if self.security.source.is_native_v8() {
            let root = self.allowed.parent().and_then(Path::parent)
                .ok_or_else(|| anyhow::anyhow!("basic original root absent"))?;
            crate::basic_witness::verify_readable_positive(root, nonce)?;
        }
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
    #[test]
    fn rejected_page_exports_only_finite_diagnostics_and_accepts_no_events() {
        let stderr = serde_json::to_vec(&serde_json::json!({
            "status":"FAIL_PRESERVED_NO_AUTOMATIC_RETRY",
            "message":"network result is not explicit WSAEACCES; Unknown: connection timed out",
            "untrusted_extra":"PRIVATE_DIAGNOSTIC_SENTINEL"
        })).unwrap();
        let mut rejected = page(vec![ProcessEvent { seq: 1, kind: EventKind::Output {
            stream: OutputStream::Stderr, chunk: stderr } },
            ProcessEvent { seq: 2, kind: EventKind::Exited { exit_code: 1, sandbox_denied: None } },
            ProcessEvent { seq: 3, kind: EventKind::Closed }], Some(1), true);
        rejected.failure = Some("PRIVATE_PROVIDER_SENTINEL".into());
        let mut evidence = TerminalEvidence::default();
        let error = evidence.accept(rejected).unwrap_err().to_string();
        assert!(error.contains("NETWORK_TIMEOUT_NOT_DENIAL_PROOF"));
        assert!(!error.contains("PRIVATE_DIAGNOSTIC_SENTINEL"));
        assert!(!error.contains("PRIVATE_PROVIDER_SENTINEL"));
        assert_eq!(evidence.after_seq, 0);
        assert!(evidence.stdout.is_empty() && evidence.stderr.is_empty());
        assert!(!evidence.saw_exit && !evidence.saw_closed);
    }
    #[test]
    fn failed_child_diagnostic_is_bounded_and_never_interprets_arbitrary_messages() {
        let unknown = br#"{"status":"FAIL_PRESERVED_NO_AUTOMATIC_RETRY","message":"PRIVATE_SENTINEL"}"#;
        assert_eq!(classify_failed_child(unknown), "UNCLASSIFIED");
        assert_eq!(classify_failed_child(b"not-json"), "UNCLASSIFIED");
        let evidence = TerminalEvidence { stderr: vec![b'a'; 65536], ..TerminalEvidence::default() };
        let rejected = page(vec![ProcessEvent { seq: 1, kind: EventKind::Output {
            stream: OutputStream::Stderr, chunk: unknown.to_vec() } }], None, false);
        let diagnostic = evidence.rejected_page_diagnostic(&rejected);
        assert_eq!(diagnostic["child_predicate"], "UNCLASSIFIED_OUTPUT_BOUND");
        assert_eq!(diagnostic["accepted_terminal_evidence"], false);
        assert!(!diagnostic.to_string().contains("PRIVATE_SENTINEL"));
    }
}
