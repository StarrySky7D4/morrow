//! Original live-owner journal writes. Persisted rows never create a live task.
use super::{MetadataPhase, MetadataReview, UploadResult, UploadStart};
use crate::{Result, WorkbenchState, product_gate::ProductGate};
use morrow_blob_upload_request_v1::Phase;
use morrow_core::{
    blob_upload_journal::{
        Execution, MetadataCompletion, OriginalResult, Outcome, Record, Request,
    },
    dispatch::HostBinding,
    store::BlobStageStoreBinding,
};
use morrow_plugin_runtime::{Fault, blob_upload_worker::Error};
use sha2::{Digest, Sha256};

#[cfg(any(
    feature = "blob-upload-ui-observe-before-qualification",
    feature = "blob-upload-ui-observe-after-qualification",
    feature = "blob-upload-ui-ack-before-qualification",
    feature = "blob-upload-ui-ack-after-qualification"
))]
#[path = "blob_upload_metadata_ui_qualification.rs"]
mod ui_qualification;

pub(super) fn qualification_failure(_point: &str) -> Result<()> {
    #[cfg(any(
        feature = "blob-upload-ui-observe-before-qualification",
        feature = "blob-upload-ui-observe-after-qualification",
        feature = "blob-upload-ui-ack-before-qualification",
        feature = "blob-upload-ui-ack-after-qualification"
    ))]
    if ui_qualification::selected(_point) {
        return Err("qualified full-UI metadata failure; original task retained".into());
    }
    #[cfg(feature = "fault-injection")]
    if std::env::var("MORROW_UPLOAD_JOURNAL_FAILURE").as_deref() == Ok(_point) {
        return Err("injected upload metadata failure; original task retained".into());
    }
    Ok(())
}

pub(super) fn request(value: &UploadStart) -> Result<Request> {
    let mut hash = Sha256::new();
    hash.update(b"morrow.original-upload-request.v1\0");
    for bytes in [
        value.submission.as_slice(),
        value.package_id.as_bytes(),
        &value.digest,
        &value.nomination_ref,
        &value.spec.owner_domain,
        &value.spec.identity.transfer_epoch,
        &value.spec.identity.object_ref,
        &value.spec.identity.operation_id,
        &value.spec.sha256,
        value.input.bytes(),
    ] {
        hash.update((bytes.len() as u64).to_le_bytes());
        hash.update(bytes);
    }
    for number in [
        value.revision,
        value.spec.byte_length,
        value.spec.limits.max_payload_bytes,
        value.spec.limits.max_wire_bytes,
        value.spec.limits.max_request_bytes,
        value.spec.limits.max_response_bytes,
        value.spec.limits.max_requests,
        value.spec.limits.max_receipts,
        value.lifetime.as_secs(),
        u64::from(value.lifetime.subsec_nanos()),
        value.limits.fuel,
        u64::try_from(value.limits.memory_bytes)?,
        u64::from(value.limits.host_calls),
    ] {
        hash.update(number.to_le_bytes());
    }
    let result = Request {
        submission: value.submission,
        package_id: value.package_id.clone(),
        package_digest: value.digest,
        registry_revision: value.revision,
        nomination_ref: value.nomination_ref,
        command: value.spec.command(),
        handler: value
            .input
            .transform()
            .ok_or("upload transform required")?
            .handler
            .clone(),
        input_sha256: Sha256::digest(value.input.bytes()).into(),
        binding_sha256: hash.finalize().into(),
    };
    result.validate()?;
    Ok(result)
}

pub(super) fn error(value: Error) -> u16 {
    match value {
        Error::Denied => 1,
        Error::Limit => 2,
        Error::Protocol => 3,
        Error::Cancelled => 4,
        Error::Expired => 5,
        Error::Storage => 6,
        Error::Unknown => 7,
        Error::Owner => 8,
        Error::Spawn => 9,
        Error::Consumed => 10,
    }
}
fn fault(value: &Fault) -> u16 {
    match value {
        Fault::TaskProtocol => 1,
        Fault::Deadline => 2,
        Fault::PackageBinding => 3,
        Fault::InactiveConnection => 4,
        Fault::InvalidModule => 5,
        Fault::UnsupportedAbi => 6,
        Fault::Limits => 7,
        Fault::Cancelled => 8,
        Fault::Trap => 9,
    }
}
fn original(value: &UploadResult) -> Option<OriginalResult> {
    Some(OriginalResult {
        outcome: match value.outcome {
            Ok(Phase::Absent) => Outcome::Absent,
            Ok(Phase::Staged) => Outcome::Staged,
            Ok(Phase::Released) => Outcome::Released,
            Ok(Phase::Ready | Phase::Receiving) => return None,
            Err(value) => Outcome::Error(error(value)),
        },
        unknown: value.unknown,
        calls: value.usage.calls,
        wire_bytes: value.usage.wire_bytes,
        native_work_bytes: value.usage.native_work_bytes,
        execution: value.execution.as_ref().map(|report| Execution {
            outcome: report.outcome.as_ref().copied().map_err(fault),
            host_calls: report.host_calls,
        }),
    })
}
pub(super) fn guard(
    gate: &ProductGate,
    binding: &BlobStageStoreBinding,
) -> morrow_core::Result<()> {
    binding.check_live()?;
    gate.check()
        .map_err(|_| morrow_core::Error::Invalid("upload history gate closed"))
}
fn owner_matches(
    owner: &WorkbenchState,
    host: HostBinding,
    binding: &BlobStageStoreBinding,
) -> Result<()> {
    if owner.host.binding() != host {
        return Err("original upload owner unavailable".into());
    }
    owner
        .host
        .store_local()
        .validate_blob_stage_store_binding(binding)?;
    Ok(())
}

pub(super) struct History {
    request: Request,
    reserved_at: i64,
    joined: bool,
    original: Option<OriginalResult>,
    observed_at: Option<i64>,
    observation_persisted: bool,
    acknowledgement_at: Option<i64>,
    acknowledgement_persisted: bool,
    review: Option<(MetadataReview, Record)>,
    completion_attempted: [bool; 2],
}
impl History {
    pub(super) fn reserved(record: Record) -> Self {
        Self {
            request: record.request().clone(),
            reserved_at: record.reserved_at_unix_ms(),
            joined: false,
            original: None,
            observed_at: None,
            observation_persisted: false,
            acknowledgement_at: None,
            acknowledgement_persisted: false,
            review: None,
            completion_attempted: [false; 2],
        }
    }
    pub(super) fn record_exit(&mut self, result: &UploadResult) {
        self.joined = true;
        self.original = original(result);
    }
    pub(super) fn needs_review(&self) -> bool {
        (self.joined && !self.observation_persisted)
            || (self.acknowledgement_at.is_some() && !self.acknowledgement_persisted)
    }
    /// Review the live owner's exact retained attempt without joining a worker,
    /// writing metadata or reconstructing authority from a historical row.
    pub(super) fn review_metadata(
        &mut self,
        owner: &WorkbenchState,
        host: HostBinding,
        binding: &BlobStageStoreBinding,
        gate: &ProductGate,
    ) -> Result<MetadataReview> {
        owner_matches(owner, host, binding)?;
        guard(gate, binding)?;
        if !self.joined {
            return Err("original upload has not joined".into());
        }
        let original = self.original.ok_or("original upload result unavailable")?;
        let observed_at = self
            .observed_at
            .ok_or("original observation time unavailable")?;
        let phase = if self.acknowledgement_at.is_some() && self.observation_persisted {
            MetadataPhase::Acknowledgement
        } else if !self.observation_persisted && self.acknowledgement_at.is_none() {
            MetadataPhase::Observation
        } else {
            return Err("no retained original metadata attempt".into());
        };
        if self.completion_attempted[phase as usize] {
            return Err("original metadata completion already attempted; observation only".into());
        }
        let row = owner
            .host
            .store_local()
            .inspect_blob_upload_local_authorized(&self.request.submission, || {
                guard(gate, binding)
            })?
            .ok_or("original reservation unavailable")?;
        let base_matches =
            row.request() == &self.request && row.reserved_at_unix_ms() == self.reserved_at;
        let observation_matches = row.original_result() == Some(original)
            && row.observed_at_unix_ms() == Some(observed_at);
        let write_required = match phase {
            MetadataPhase::Observation if base_matches && row.revision() == 1 => true,
            MetadataPhase::Observation
                if base_matches && observation_matches && row.revision() == 2 =>
            {
                false
            }
            MetadataPhase::Acknowledgement
                if base_matches && observation_matches && row.revision() == 2 =>
            {
                true
            }
            MetadataPhase::Acknowledgement
                if base_matches
                    && observation_matches
                    && row.revision() == 3
                    && row.acknowledged_at_unix_ms() == self.acknowledgement_at =>
            {
                false
            }
            _ => return Err("original metadata changed; review refused".into()),
        };
        if let Some((review, previous)) = &self.review {
            if *previous != row || review.phase != phase {
                return Err("original metadata changed after review".into());
            }
            guard(gate, binding)?;
            return Ok(review.clone());
        }
        let mut permit = [0; 32];
        getrandom::fill(&mut permit)?;
        if permit == [0; 32] {
            return Err("metadata review identity unavailable".into());
        }
        let review = MetadataReview {
            submission: self.request.submission,
            permit,
            phase,
            write_required,
            original,
            observed_at,
            acknowledgement_at: self.acknowledgement_at,
        };
        guard(gate, binding)?;
        self.review = Some((review.clone(), row));
        Ok(review)
    }
    pub(super) fn complete_metadata(
        &mut self,
        owner: &mut WorkbenchState,
        host: HostBinding,
        binding: &BlobStageStoreBinding,
        gate: &ProductGate,
        permit: [u8; 32],
    ) -> Result<MetadataPhase> {
        let (review, _) = self.review.as_ref().ok_or("metadata review required")?;
        if permit != review.permit || self.completion_attempted[review.phase as usize] {
            return Err("metadata review does not authorize this completion".into());
        }
        // Consume before any fallible check/write. Missing replies never permit
        // another completion attempt, even if the original record is unchanged.
        let (review, expected) = self.review.take().expect("review checked");
        self.completion_attempted[review.phase as usize] = true;
        owner_matches(owner, host, binding)?;
        guard(gate, binding)?;
        if review.write_required {
            let completion = match review.phase {
                MetadataPhase::Observation => MetadataCompletion::Observation {
                    result: review.original,
                    at: review.observed_at,
                },
                MetadataPhase::Acknowledgement => MetadataCompletion::Acknowledgement {
                    at: review
                        .acknowledgement_at
                        .ok_or("original acknowledgement unavailable")?,
                },
            };
            owner.host.prepare_write()?;
            completion_failure("before")?;
            owner
                .host
                .store_local_mut()
                .complete_blob_upload_metadata_local_authorized(&expected, completion, || {
                    guard(gate, binding)
                })?;
            completion_failure("after")?;
        }
        // Also re-check the exact full original result/timestamps when the
        // original attempt committed but its response was lost. No write then.
        self.reconcile(owner, host, binding, gate)?;
        Ok(review.phase)
    }
    pub(super) fn persist_observation(
        &mut self,
        owner: &mut WorkbenchState,
        host: HostBinding,
        binding: &BlobStageStoreBinding,
        gate: &ProductGate,
    ) -> Result<()> {
        if !self.joined || self.observed_at.is_some() {
            return Err("original upload result is unavailable or already attempted".into());
        }
        let result = self
            .original
            .ok_or("original upload result is not terminal")?;
        let now = crate::platform::unix_millis()?;
        self.observed_at = Some(now);
        owner_matches(owner, host, binding)?;
        guard(gate, binding)?;
        owner.host.prepare_write()?;
        qualification_failure("observe-before")?;
        owner
            .host
            .store_local_mut()
            .observe_blob_upload_local_authorized(&self.request, result, now, || {
                guard(gate, binding)
            })?;
        qualification_failure("observe-after")?;
        gate.check()?;
        self.observation_persisted = true;
        Ok(())
    }
    /// An explicit read on the same original owner can recognize a committed
    /// metadata attempt. A missing row never dispatches or repeats a write.
    pub(super) fn reconcile(
        &mut self,
        owner: &WorkbenchState,
        host: HostBinding,
        binding: &BlobStageStoreBinding,
        gate: &ProductGate,
    ) -> Result<()> {
        owner_matches(owner, host, binding)?;
        let row = owner
            .host
            .store_local()
            .inspect_blob_upload_local_authorized(&self.request.submission, || {
                guard(gate, binding)
            })?
            .ok_or("original upload history missing")?;
        if !self.joined
            || self.original.is_none()
            || self.observed_at.is_none()
            || row.request() != &self.request
            || row.reserved_at_unix_ms() != self.reserved_at
            || row.original_result() != self.original
            || row.observed_at_unix_ms() != self.observed_at
            || row.acknowledged_at_unix_ms() != self.acknowledgement_at
        {
            return Err(
                "original upload metadata remains unresolved; no automatic write or retry".into(),
            );
        }
        gate.check()?;
        self.review = None;
        self.observation_persisted = true;
        self.acknowledgement_persisted = self.acknowledgement_at.is_some();
        Ok(())
    }
    pub(super) fn acknowledge(
        &mut self,
        owner: &mut WorkbenchState,
        host: HostBinding,
        binding: &BlobStageStoreBinding,
        gate: &ProductGate,
    ) -> Result<()> {
        if self.acknowledgement_persisted {
            owner_matches(owner, host, binding)?;
            guard(gate, binding)?;
            return Ok(());
        }
        if !self.observation_persisted || self.acknowledgement_at.is_some() {
            return Err("original upload history requires explicit reconciliation".into());
        }
        // An earlier observation review cannot authorize work after this new
        // explicit acknowledgement changes the retained attempt.
        self.review = None;
        let now = crate::platform::unix_millis()?;
        self.acknowledgement_at = Some(now);
        owner_matches(owner, host, binding)?;
        guard(gate, binding)?;
        owner.host.prepare_write()?;
        qualification_failure("ack-before")?;
        owner
            .host
            .store_local_mut()
            .acknowledge_blob_upload_local_authorized(&self.request, now, || {
                guard(gate, binding)
            })?;
        qualification_failure("ack-after")?;
        gate.check()?;
        self.acknowledgement_persisted = true;
        Ok(())
    }
}

fn completion_failure(_point: &str) -> Result<()> {
    #[cfg(feature = "fault-injection")]
    if std::env::var("MORROW_UPLOAD_METADATA_COMPLETION_FAILURE").as_deref() == Ok(_point) {
        return Err("injected metadata completion failure; original task retained".into());
    }
    Ok(())
}
