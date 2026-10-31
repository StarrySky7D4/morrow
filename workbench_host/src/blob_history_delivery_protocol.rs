//! Private original-owner controls for one explicitly reviewed history packet.
//! These actions never enter the scheduler's maintenance/reclaim path.
use super::wire;
use crate::{Result, Workbench, blob_history_delivery::*};
use capnp::{message::Builder, private::layout::StructReader, traits::IntoInternalStructReader};
use std::time::Duration;

fn identity(bytes: &[u8]) -> Result<[u8; 32]> {
    let value: [u8; 32] = bytes.try_into()?;
    if value == [0; 32] {
        return Err("empty history delivery identity".into());
    }
    Ok(value)
}

fn bounded_text(value: capnp::Result<capnp::text::Reader<'_>>) -> Result<String> {
    let value = value?.to_str()?;
    if value.is_empty() || value.len() > 256 || value.chars().any(char::is_control) {
        return Err("invalid history delivery text".into());
    }
    Ok(value.to_owned())
}

// The private schema digest is exact. Reject truncated/extended structs,
// hidden scalar bits and unrelated pointer fields rather than silently ignoring
// them. A generated template avoids assumptions about pointer-slot ordinals.
fn shape(actual: StructReader<'_>, expected: StructReader<'_>) -> Result<()> {
    if actual.get_data_section_size() != expected.get_data_section_size()
        || actual.get_pointer_section_size() != expected.get_pointer_section_size()
        || actual.get_data_section_as_blob() != expected.get_data_section_as_blob()
        || (0..usize::from(expected.get_pointer_section_size())).any(|index| {
            actual.get_pointer_field(index).is_null() != expected.get_pointer_field(index).is_null()
        })
    {
        return Err("unexpected history delivery request shape".into());
    }
    Ok(())
}

fn request(value: wire::blob_history_delivery_request::Reader<'_>) -> Result<DeliveryRequest> {
    let timeout = value.get_timeout_ms();
    if timeout == 0 || timeout > 120_000 || value.total_size()?.cap_count != 0 {
        return Err("invalid history delivery request bounds".into());
    }
    let result = DeliveryRequest {
        request_id: identity(value.get_request_id()?)?,
        submission: identity(value.get_submission()?)?,
        package_id: bounded_text(value.get_package_id())?,
        package_digest: identity(value.get_package_digest()?)?,
        registry_revision: value.get_registry_revision(),
        handler: bounded_text(value.get_handler())?,
        output_type: bounded_text(value.get_output_type())?,
        lifetime: Duration::from_millis(u64::from(timeout)),
    };
    let mut expected = Builder::new_default();
    write_request(&result, expected.init_root());
    shape(
        value.into_internal_struct_reader(),
        expected
            .get_root_as_reader::<wire::blob_history_delivery_request::Reader>()?
            .into_internal_struct_reader(),
    )?;
    Ok(result)
}

fn approval(value: wire::request::Reader<'_>) -> Result<DeliveryApproval> {
    let key = DeliveryKey::from_bytes(value.get_blob_history_delivery_key()?)?;
    let fields = value.get_blob_history_delivery_approval()?;
    let result = DeliveryApproval {
        key,
        packet_sha256: identity(fields.get_packet_sha256()?)?,
        package_digest: identity(fields.get_package_digest()?)?,
        registry_revision: fields.get_registry_revision(),
    };
    let mut expected = Builder::new_default();
    {
        let mut out = expected.init_root::<wire::blob_history_delivery_approval::Builder>();
        out.set_packet_sha256(&result.packet_sha256);
        out.set_package_digest(&result.package_digest);
        out.set_registry_revision(result.registry_revision);
    }
    shape(
        fields.into_internal_struct_reader(),
        expected
            .get_root_as_reader::<wire::blob_history_delivery_approval::Reader>()?
            .into_internal_struct_reader(),
    )?;
    Ok(result)
}

pub(super) fn validate(value: wire::request::Reader<'_>) -> Result<()> {
    use wire::Action::*;
    if value.total_size()?.cap_count != 0 {
        return Err("history delivery capabilities are forbidden".into());
    }
    let action = value.get_action()?;
    let mut expected = Builder::new_default();
    {
        let mut out = expected.init_root::<wire::request::Builder>();
        out.set_version(1);
        out.set_digest(&super::digest());
        out.set_action(action);
        match action {
            BlobHistoryDeliveryPrepare => {
                request(value.get_blob_history_delivery_request()?)?;
                out.init_blob_history_delivery_request();
            }
            BlobHistoryDeliveryApprove => {
                approval(value)?;
                out.reborrow().init_blob_history_delivery_approval();
                out.set_blob_history_delivery_key(&[1]);
            }
            BlobHistoryDeliveryPreview
            | BlobHistoryDeliveryRead
            | BlobHistoryDeliveryCancel
            | BlobHistoryDeliveryAcknowledge => {
                DeliveryKey::from_bytes(value.get_blob_history_delivery_key()?)?;
                out.set_blob_history_delivery_key(&[1]);
            }
            BlobHistoryDeliveryDiscover => {
                identity(value.get_blob_history_delivery_request_id()?)?;
                out.set_blob_history_delivery_request_id(&[1]);
            }
            _ => return Err("not a history delivery action".into()),
        }
    }
    shape(
        value.into_internal_struct_reader(),
        expected
            .get_root_as_reader::<wire::request::Reader>()?
            .into_internal_struct_reader(),
    )
}

fn error(value: DeliveryError) -> u16 {
    match value {
        DeliveryError::Cancelled => 1,
        DeliveryError::Expired => 2,
        DeliveryError::StaleOwner => 3,
        DeliveryError::StaleSelection => 4,
        DeliveryError::HistoryChanged => 5,
        DeliveryError::GateClosed => 6,
        DeliveryError::Preparation => 7,
        DeliveryError::Execution => 8,
        DeliveryError::PluginFailure => 9,
        DeliveryError::Protocol => 10,
        DeliveryError::Unwind => 11,
        DeliveryError::Close => 12,
    }
}

fn close_error(value: &morrow_core::Error) -> (u16, &str) {
    use morrow_core::Error::*;
    match value {
        Limit => (1, ""),
        Invalid(detail) => (2, detail),
        UnsupportedVersion => (3, ""),
        RevisionConflict => (4, ""),
        Integrity => (5, ""),
        Storage => (6, ""),
        Io => (7, ""),
        Retained => (8, ""),
        StorageBusy => (9, ""),
        StorageFull => (10, ""),
        OperationConflict => (11, ""),
        NotFound => (12, ""),
        EvidenceUnavailable => (13, ""),
        EventCapacity => (14, ""),
        ArchiveCapacity => (15, ""),
        CommitUnknown => (16, ""),
    }
}

fn write_request(
    value: &DeliveryRequest,
    mut out: wire::blob_history_delivery_request::Builder<'_>,
) {
    out.set_request_id(&value.request_id);
    out.set_submission(&value.submission);
    out.set_package_id(value.package_id.as_str());
    out.set_package_digest(&value.package_digest);
    out.set_registry_revision(value.registry_revision);
    out.set_handler(value.handler.as_str());
    out.set_output_type(value.output_type.as_str());
    out.set_timeout_ms(value.lifetime.as_millis() as u32);
}

fn preview(value: &DeliveryPreview, mut out: wire::blob_history_delivery_preview::Builder<'_>) {
    out.set_present(true);
    out.set_key(value.key.as_bytes());
    write_request(&value.request, out.reborrow().init_request());
    out.set_task_id(value.task_id.as_str());
    out.set_packet(&value.packet);
    out.set_packet_sha256(&value.packet_sha256);
}

fn receipt(value: &DeliveryReceipt, mut out: wire::blob_history_delivery_receipt::Builder<'_>) {
    out.set_found(true);
    out.set_key(value.key.as_bytes());
    out.set_request_id(&value.request_id);
    out.set_consumed(value.consumed);
    out.set_dispatched(value.dispatched);
    out.set_finished(value.finished);
    out.set_acknowledged(value.acknowledged);
    out.set_close(match value.close {
        CloseState::NotCreated => 0,
        CloseState::Pending => 1,
        CloseState::Closed => 2,
        CloseState::Failed => 3,
        CloseState::Unknown => 4,
    });
    if let Some(failure) = &value.close_failure {
        match failure {
            CloseFailure::Core(value) => {
                let (code, detail) = close_error(value);
                out.set_close_failure_kind(1);
                out.set_close_core_error(code);
                out.set_close_detail(detail);
            }
            CloseFailure::Unwind => out.set_close_failure_kind(2),
        }
    }
    out.set_error(value.error.map_or(0, error));
    out.set_delivery_error(value.delivery_error.map_or(0, error));
    if let Some(execution) = &value.execution {
        out.set_has_execution(true);
        match &execution.outcome {
            Ok(code) => out.set_exit_code(*code),
            Err(fault) => out.set_execution_fault(super::blob_upload::fault(fault.clone())),
        }
        out.set_host_calls(execution.host_calls);
        out.set_fuel_remaining(execution.fuel_remaining);
    }
    if let Some(failure) = &value.failure {
        out.set_has_plugin_failure(true);
        out.set_plugin_failure_code(failure.code as u16);
    }
    if let Some(output) = &value.output {
        out.set_has_output(true);
        out.set_output_type(output.type_id.as_str());
        out.set_output(&output.bytes);
    }
}

pub(super) fn handle(
    host: &mut Workbench,
    value: wire::request::Reader<'_>,
    mut out: wire::response::Builder<'_>,
) -> Result<()> {
    use wire::Action::*;
    // Explicit absent envelopes are distinct from a malformed/lost reply. They
    // do not establish that any prior upload or disclosure failed.
    out.reborrow().init_blob_history_delivery_preview();
    out.reborrow().init_blob_history_delivery_receipt();
    match value.get_action()? {
        BlobHistoryDeliveryPrepare => {
            let prepared = host.prepare_blob_history_delivery(request(
                value.get_blob_history_delivery_request()?,
            )?)?;
            preview(&prepared, out.init_blob_history_delivery_preview());
        }
        BlobHistoryDeliveryPreview => {
            let prepared = host.preview_blob_history_delivery(DeliveryKey::from_bytes(
                value.get_blob_history_delivery_key()?,
            )?)?;
            preview(&prepared, out.init_blob_history_delivery_preview());
        }
        BlobHistoryDeliveryDiscover => {
            if let Some(original) = host.discover_blob_history_delivery(&identity(
                value.get_blob_history_delivery_request_id()?,
            )?)? {
                receipt(&original, out.init_blob_history_delivery_receipt());
            }
        }
        action => {
            let key = DeliveryKey::from_bytes(value.get_blob_history_delivery_key()?)?;
            let original = match action {
                BlobHistoryDeliveryApprove => {
                    host.approve_blob_history_delivery(approval(value)?)?
                }
                BlobHistoryDeliveryRead => host.read_blob_history_delivery(key)?,
                BlobHistoryDeliveryCancel => host.cancel_blob_history_delivery(key)?,
                BlobHistoryDeliveryAcknowledge => {
                    host.acknowledge_blob_history_delivery(key)?;
                    host.read_blob_history_delivery(key)?
                }
                _ => return Err("not a history delivery action".into()),
            };
            receipt(&original, out.init_blob_history_delivery_receipt());
        }
    }
    Ok(())
}
