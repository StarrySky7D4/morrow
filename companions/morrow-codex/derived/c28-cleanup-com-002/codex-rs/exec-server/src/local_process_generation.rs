//! Object-generation fences share the original RPC implementation.
//! RPC wrappers pass None and continue to select the current process by ID.
use std::sync::Arc;
use std::time::Duration;

use codex_exec_server_protocol::JSONRPCErrorError;
use tokio::sync::watch;

use super::LocalProcess;
use super::ProcessEntry;
use super::checked_lifecycle;
use super::pty_process_signal;
use crate::protocol::ProcessOutputChunk;
use crate::protocol::ReadParams;
use crate::protocol::ReadResponse;
use crate::protocol::SignalParams;
use crate::protocol::SignalResponse;
use crate::protocol::TerminateParams;
use crate::protocol::TerminateResponse;
use crate::protocol::WriteParams;
use crate::protocol::WriteResponse;
use crate::protocol::WriteStatus;
use crate::rpc::internal_error;
use crate::rpc::invalid_params;
use crate::rpc::invalid_request;

fn stale() -> JSONRPCErrorError {
    invalid_request("original process generation is unavailable".into())
}

#[cfg(test)]
#[path = "local_process_generation_tests.rs"]
mod tests;

impl LocalProcess {
    pub(super) async fn read_generation(
        &self,
        params: ReadParams,
        expected_wake: Option<&watch::Sender<u64>>,
    ) -> Result<ReadResponse, JSONRPCErrorError> {
        let after_seq = params.after_seq.unwrap_or(0);
        let max_bytes = params.max_bytes.unwrap_or(usize::MAX);
        let wait = Duration::from_millis(params.wait_ms.unwrap_or(0));
        let deadline = tokio::time::Instant::now() + wait;

        loop {
            let (response, output_notify) = {
                let mut process_map = self.inner.processes.lock().await;
                let process = process_map.get_mut(&params.process_id).ok_or_else(|| {
                    invalid_request(format!("unknown process id {}", params.process_id))
                })?;
                let ProcessEntry::Running(process) = process else {
                    return Err(invalid_request(format!(
                        "process id {} is starting",
                        params.process_id
                    )));
                };

                if expected_wake.is_some_and(|expected| !process.wake_tx.same_channel(expected)) {
                    return Err(stale());
                }
                let expected_wake = process.wake_tx.clone();
                checked_lifecycle::veto(process, &expected_wake);
                let mut chunks = Vec::new();
                let mut total_bytes = 0;
                let mut next_seq = process.next_seq;
                for retained in process.output.iter().filter(|chunk| chunk.seq > after_seq) {
                    let chunk_len = retained.chunk.len();
                    if !chunks.is_empty() && total_bytes + chunk_len > max_bytes {
                        break;
                    }
                    total_bytes += chunk_len;
                    chunks.push(ProcessOutputChunk {
                        seq: retained.seq,
                        stream: retained.stream,
                        chunk: retained.chunk.clone().into(),
                    });
                    next_seq = retained.seq + 1;
                    if total_bytes >= max_bytes {
                        break;
                    }
                }
                if params.max_bytes.is_none() {
                    next_seq = process.next_seq;
                }
                (
                    ReadResponse {
                        chunks,
                        next_seq,
                        exited: process.exit_code.is_some(),
                        exit_code: process.exit_code,
                        closed: process.closed,
                        failure: process.lifecycle.failure().map(str::to_owned),
                        sandbox_denied: process.sandbox_denied,
                    },
                    Arc::clone(&process.output_notify),
                )
            };

            let has_new_terminal_event =
                response.exited && after_seq < response.next_seq.saturating_sub(1);
            if !response.chunks.is_empty()
                || response.closed
                || response.failure.is_some()
                || has_new_terminal_event
                || tokio::time::Instant::now() >= deadline
            {
                let _total_bytes: usize = response
                    .chunks
                    .iter()
                    .map(|chunk| chunk.chunk.0.len())
                    .sum();
                return Ok(response);
            }

            let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
            if remaining.is_zero() {
                return Ok(response);
            }
            let _ = tokio::time::timeout(remaining, output_notify.notified()).await;
        }
    }

    pub(super) async fn write_generation(
        &self,
        params: WriteParams,
        expected_wake: Option<&watch::Sender<u64>>,
    ) -> Result<WriteResponse, JSONRPCErrorError> {
        let _input_bytes = params.chunk.0.len();
        if params.write_id.is_empty() {
            return Err(invalid_params("writeId must not be empty".to_string()));
        }

        let (writer_tx, accepted_stdin_write_ids) = {
            let process_map = self.inner.processes.lock().await;
            let Some(process) = process_map.get(&params.process_id) else {
                if expected_wake.is_some() {
                    return Err(stale());
                }
                return Ok(WriteResponse {
                    status: WriteStatus::UnknownProcess,
                });
            };
            let ProcessEntry::Running(process) = process else {
                if expected_wake.is_some() {
                    return Err(stale());
                }
                return Ok(WriteResponse {
                    status: WriteStatus::Starting,
                });
            };
            if expected_wake.is_some_and(|expected| !process.wake_tx.same_channel(expected)) {
                return Err(stale());
            }
            if !process.tty && !process.pipe_stdin {
                return Ok(WriteResponse {
                    status: WriteStatus::StdinClosed,
                });
            }
            (
                process.session.writer_sender(),
                Arc::clone(&process.accepted_stdin_write_ids),
            )
        };

        // Keep the original accepted-write gate through reserve/send, so a
        // checked close cannot overtake an accepted write or admit a new write
        // after its sticky closing transition. This is not the process-map lock.
        let mut accepted_stdin_write_ids = accepted_stdin_write_ids.lock().await;
        if accepted_stdin_write_ids.contains(&params.write_id) {
            return Ok(WriteResponse {
                status: WriteStatus::Accepted,
            });
        }
        if accepted_stdin_write_ids.input_closing {
            return Ok(WriteResponse {
                status: WriteStatus::StdinClosed,
            });
        }
        let permit = writer_tx
            .reserve()
            .await
            .map_err(|_| internal_error("failed to write to process stdin".to_string()))?;

        // After this synchronous send, record the write id before any further await.
        // Otherwise a cancelled RPC handler could retry and write the same bytes again.
        permit.send(params.chunk.into_inner());
        accepted_stdin_write_ids.remember(params.write_id);

        Ok(WriteResponse {
            status: WriteStatus::Accepted,
        })
    }

    pub(super) async fn signal_generation(
        &self,
        params: SignalParams,
        expected_wake: Option<&watch::Sender<u64>>,
    ) -> Result<SignalResponse, JSONRPCErrorError> {
        let session = {
            let process_map = self.inner.processes.lock().await;
            match process_map.get(&params.process_id) {
                Some(ProcessEntry::Running(process)) => {
                    if expected_wake.is_some_and(|expected| !process.wake_tx.same_channel(expected))
                    {
                        return Err(stale());
                    }
                    if process.exit_code.is_some() {
                        return Ok(SignalResponse {});
                    }
                    Some(Arc::clone(&process.session))
                }
                Some(ProcessEntry::Starting(_)) | None => {
                    if expected_wake.is_some() {
                        return Err(stale());
                    }
                    None
                }
            }
        };
        if let Some(session) = session {
            session
                .signal(pty_process_signal(params.signal))
                .map_err(|err| internal_error(format!("failed to signal process: {err}")))?;
        }

        Ok(SignalResponse {})
    }

    pub(super) async fn terminate_generation(
        &self,
        params: TerminateParams,
        expected_wake: Option<&watch::Sender<u64>>,
    ) -> Result<TerminateResponse, JSONRPCErrorError> {
        let (running, session) = {
            let mut process_map = self.inner.processes.lock().await;
            match process_map.get_mut(&params.process_id) {
                Some(ProcessEntry::Running(process)) => {
                    if expected_wake.is_some_and(|expected| !process.wake_tx.same_channel(expected))
                    {
                        return Err(stale());
                    }
                    if let Some(network_policy_shutdown) = &process.network_policy_shutdown {
                        network_policy_shutdown.cancel();
                    }
                    if process.exit_code.is_some() {
                        return Ok(TerminateResponse { running: false });
                    }
                    process.termination_requested = true;
                    (true, Some(Arc::clone(&process.session)))
                }
                Some(ProcessEntry::Starting(_)) => {
                    if expected_wake.is_some() {
                        return Err(stale());
                    }
                    process_map.remove(&params.process_id);
                    (true, None)
                }
                None => {
                    if expected_wake.is_some() {
                        return Err(stale());
                    }
                    (false, None)
                }
            }
        };
        if let Some(session) = session {
            #[cfg(windows)]
            session.request_terminate();
            #[cfg(not(windows))]
            session.terminate();
        }

        Ok(TerminateResponse { running })
    }
}
