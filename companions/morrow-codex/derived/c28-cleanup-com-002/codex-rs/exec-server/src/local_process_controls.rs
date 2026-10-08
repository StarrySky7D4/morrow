//! Checked controls for the exact session retained by LocalProcess.
//! No Core/owner lock or process-map lock is held across an applied-result wait.

use std::sync::Arc;

use codex_utils_pty::TerminalSize;
use tokio::sync::watch;

use super::LocalProcess;
use super::ProcessEntry;
use crate::ExecServerError;
use crate::ProcessId;
use crate::process::ProcessControlCapabilities;
use crate::process::ProcessControlOutcome;

impl LocalProcess {
    pub(super) async fn checked_process_capabilities(
        &self,
        process_id: &ProcessId,
        expected_wake: &watch::Sender<u64>,
    ) -> Result<ProcessControlCapabilities, ExecServerError> {
        let mut processes = self.inner.processes.lock().await;
        let Some(ProcessEntry::Running(process)) = processes.get_mut(process_id) else {
            return Ok(ProcessControlCapabilities::default());
        };
        if !super::checked_lifecycle::veto(process, expected_wake)
            || process.exit_code.is_some()
            || process.termination_requested
        {
            return Ok(ProcessControlCapabilities::default());
        }
        let supported = process.session.checked_control_capabilities();
        Ok(ProcessControlCapabilities {
            close_input: (process.tty || process.pipe_stdin) && supported.close_stdin,
            resize_pty: process.tty && supported.resize,
        })
    }

    pub(super) async fn close_process_input_checked(
        &self,
        process_id: &ProcessId,
        expected_wake: &watch::Sender<u64>,
    ) -> Result<ProcessControlOutcome, ExecServerError> {
        let (session, accepted_ids) = {
            let mut processes = self.inner.processes.lock().await;
            let Some(ProcessEntry::Running(process)) = processes.get_mut(process_id) else {
                return Ok(ProcessControlOutcome::Rejected);
            };
            if !super::checked_lifecycle::veto(process, expected_wake)
                || process.exit_code.is_some()
                || process.termination_requested
            {
                return Ok(ProcessControlOutcome::Rejected);
            }
            if !(process.tty || process.pipe_stdin)
                || !process.session.checked_control_capabilities().close_stdin
            {
                return Ok(ProcessControlOutcome::Unsupported);
            }
            (
                Arc::clone(&process.session),
                Arc::clone(&process.accepted_stdin_write_ids),
            )
        };
        {
            let mut accepted = accepted_ids.lock().await;
            if accepted.input_closing {
                return Ok(ProcessControlOutcome::Rejected);
            }
            // This latch remains set on error, cancellation or a lost ACK. It
            // cannot authorize a second close effect for the same process.
            accepted.input_closing = true;
        }
        session.close_stdin_checked().await.map_err(|error| {
            ExecServerError::Protocol(format!("checked input close outcome unknown: {error}"))
        })?;
        Ok(ProcessControlOutcome::Applied)
    }

    pub(super) async fn resize_process_checked(
        &self,
        process_id: &ProcessId,
        expected_wake: &watch::Sender<u64>,
        rows: u16,
        cols: u16,
    ) -> Result<ProcessControlOutcome, ExecServerError> {
        if !(1..=4096).contains(&rows) || !(1..=4096).contains(&cols) {
            return Ok(ProcessControlOutcome::Rejected);
        }
        let session = {
            let mut processes = self.inner.processes.lock().await;
            let Some(ProcessEntry::Running(process)) = processes.get_mut(process_id) else {
                return Ok(ProcessControlOutcome::Rejected);
            };
            if !super::checked_lifecycle::veto(process, expected_wake)
                || process.exit_code.is_some()
                || process.termination_requested
            {
                return Ok(ProcessControlOutcome::Rejected);
            }
            if !process.tty || !process.session.checked_control_capabilities().resize {
                return Ok(ProcessControlOutcome::Unsupported);
            }
            Arc::clone(&process.session)
        };
        session
            .resize_checked(TerminalSize { rows, cols })
            .await
            .map_err(|error| {
                ExecServerError::Protocol(format!("checked PTY resize outcome unknown: {error}"))
            })?;
        Ok(ProcessControlOutcome::Applied)
    }
}

#[cfg(test)]
#[path = "local_process_controls_tests.rs"]
mod tests;
