//! Qualification fixture and explicitly task-bound data consumer, never a history grant.
use morrow_blob_upload_history_v1::{Execution, Metadata, Outcome, Packet, Stage};
use morrow_plugin_sdk::{task::FailureCode, wasm};

fn accepted(bytes: &[u8]) -> bool {
    let Ok(packet) = Packet::decode_for(bytes, [1; 32], [9; 32]) else {
        return false;
    };
    let r = packet.record();
    r.package_id == "org.example.history"
        && r.package_digest == [2; 32]
        && r.registry_revision == u64::MAX
        && r.operation_id == "upload-原记录"
        && r.byte_length == 16 * 1024 * 1024
        && r.whole_sha256 == [3; 32]
        && r.handler == "inspect-history"
        && r.input_sha256 == [4; 32]
        && r.request_binding_sha256 == [5; 32]
        && r.reserved_at == -10
        && r.current_stage == Stage::Staged
        && r.original_unknown()
        && match r.metadata {
            Metadata::Reserved => true,
            Metadata::Acknowledged {
                result,
                observed_at,
                at,
            } => {
                result.outcome == Outcome::Error(7)
                    && result.unknown
                    && result.calls == u32::MAX
                    && result.wire_bytes == u64::MAX
                    && result.native_work_bytes == u64::MAX
                    && result.execution
                        == Some(Execution {
                            outcome: Ok(i32::MIN),
                            host_calls: u32::MAX,
                        })
                    && observed_at == 0
                    && at == 1
            }
            _ => false,
        }
}

#[unsafe(no_mangle)]
pub extern "C" fn morrow_run() -> i32 {
    let Ok(task) = wasm::read_task() else {
        return -1;
    };
    let valid = task.transform().is_some_and(|t| {
        t.input_type == "morrow.blob.upload.history.v1"
            && t.output_type == "morrow.blob.upload.history.v1"
            && match t.handler.as_str() {
                "history.fixture.inspect" => accepted(&t.input),
                "history.inspect" => Packet::decode_task(&t.input, task.task_id()).is_ok(),
                _ => false,
            }
    });
    let result = if valid {
        wasm::complete_output(&task, &task.transform().unwrap().input)
    } else {
        wasm::complete_failure(&task, FailureCode::InvalidInput, "History fixture rejected")
    };
    if result.is_ok() { 0 } else { -1 }
}
