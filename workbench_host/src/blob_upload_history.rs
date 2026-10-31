//! Public data projection of an explicitly selected original upload history.
//! The existing live library/owner/gate performs the read. This projection adds
//! no worker, Store, package grant, metadata completion or upload replay route.
use crate::{Result, Workbench, io_tasks::blob_upload::Observation};
use morrow_blob_upload_history_v1 as sdk;
use morrow_core::blob_upload_journal as original;

fn result(value: original::OriginalResult) -> sdk::OriginalResult {
    sdk::OriginalResult {
        outcome: match value.outcome {
            original::Outcome::Absent => sdk::Outcome::Absent,
            original::Outcome::Staged => sdk::Outcome::Staged,
            original::Outcome::Released => sdk::Outcome::Released,
            original::Outcome::Error(code) => sdk::Outcome::Error(code),
        },
        unknown: value.unknown,
        calls: value.calls,
        wire_bytes: value.wire_bytes,
        native_work_bytes: value.native_work_bytes,
        execution: value.execution.map(|e| sdk::Execution {
            outcome: e.outcome,
            host_calls: e.host_calls,
        }),
    }
}
impl Workbench {
    /// Read one selected record using the current protected owner. The caller
    /// must separately approve any disclosure to a plugin; schema, nonce and
    /// submission IDs are correlations, not evidence of that approval. No
    /// automatic package delivery or selection by an untrusted guest is added.
    pub fn read_blob_upload_history_packet(
        &self,
        submission: &[u8; 32],
        query_nonce: [u8; 32],
    ) -> Result<Vec<u8>> {
        if query_nonce == [0; 32] || *submission == [0; 32] {
            return Err("history selection and query nonce must be nonzero".into());
        }
        let observed = self.inspect_persisted_blob_upload(submission)?;
        let row = &observed.record;
        let request = row.request();
        let metadata = match (
            row.original_result(),
            row.observed_at_unix_ms(),
            row.acknowledged_at_unix_ms(),
        ) {
            (None, None, None) => sdk::Metadata::Reserved,
            (Some(value), Some(at), None) => sdk::Metadata::Observed {
                result: result(value),
                at,
            },
            (Some(value), Some(observed_at), Some(at)) => sdk::Metadata::Acknowledged {
                result: result(value),
                observed_at,
                at,
            },
            _ => return Err("original upload metadata is inconsistent".into()),
        };
        let packet = sdk::Packet::new(
            query_nonce,
            sdk::Record {
                submission: request.submission,
                package_id: request.package_id.clone(),
                package_digest: request.package_digest,
                registry_revision: request.registry_revision,
                operation_id: request.command.operation_id.clone(),
                byte_length: request.command.byte_length,
                whole_sha256: request.command.sha256,
                handler: request.handler.clone(),
                input_sha256: request.input_sha256,
                request_binding_sha256: request.binding_sha256,
                reserved_at: row.reserved_at_unix_ms(),
                metadata,
                current_stage: match observed.current_stage {
                    Observation::Absent => sdk::Stage::Absent,
                    Observation::Staged => sdk::Stage::Staged,
                    Observation::Released => sdk::Stage::Released,
                },
            },
        )?;
        let bytes = packet.encode()?;
        self.state.product_gate.check()?;
        Ok(bytes)
    }
}
