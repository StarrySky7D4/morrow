//! Connect live target selections to the existing durable preparation path.
use super::*;
use morrow_core::{
    file_content::{FileContent, MAX_CONTENT_BYTES},
    io_evidence::{Kind, Material},
    io_intent::Record,
};
impl TargetBroker {
    /// Compatibility wrapper for a local monotonic clock without cancellation.
    pub fn prepare_request(
        &self,
        manager: &Manager,
        host: &mut HostRuntime,
        instance: &ManagedInstance,
        request: &RequestRecord,
        clock: impl FnMut() -> u64,
    ) -> Result<Record> {
        let mut control = LocalControl(clock);
        self.prepare_request_controlled(manager, host, instance, request, &mut control)
    }

    /// Persist an exact plan only under the original target's live authority.
    /// Every call consumes admitted request bytes, including idempotent retries.
    /// Prepared is historical, not permission to perform a filesystem effect.
    /// CommitUnknown must be reconciled by reading history; never infer rollback.
    pub fn prepare_request_controlled(
        &self,
        manager: &Manager,
        host: &mut HostRuntime,
        instance: &ManagedInstance,
        request: &RequestRecord,
        control: &mut impl TargetControl,
    ) -> Result<Record> {
        self.validate_request_controlled(manager, host, instance, request, control)?;
        let lease = self.target_lease(request)?;
        let binding = lease.binding();
        let _job = controlled(control, |now| {
            Ok(binding.admit(
                manager,
                host,
                instance,
                request.request().disposition.capability(),
                0,
                request.container().len() as u64,
                now,
            )?)
        })?;
        let prepared = Record::prepared(request.command()?)?;
        let command = prepared.command();
        let material = Material::encode(
            Kind::Request,
            &command.operation_id,
            &command.subject,
            command.request_sha256,
            request.container(),
        )?;
        let mut rejection = None;
        let result = host
            .store_local_mut()
            .prepare_file_mutation_local_authorized(&prepared, &material, || {
                controlled(control, |now| {
                    binding.check_liveness(now)?;
                    Ok(())
                })
                .map_err(|error| {
                    rejection = Some(error);
                    morrow_core::Error::Invalid("inactive selected file preparation")
                })
            });
        let stored = result.map_err(|error| rejection.unwrap_or(Error::Persistence(error)))?;
        // A failed delivery check does not undo an already committed plan.
        delivery(controlled(control, |now| {
            lease.check(manager, host, instance, now)?;
            Ok(())
        }))?;
        Ok(stored)
    }

    /// Compatibility wrapper for a local monotonic clock without cancellation.
    pub fn stage_content(
        &self,
        manager: &Manager,
        host: &mut HostRuntime,
        instance: &ManagedInstance,
        request: &RequestRecord,
        bytes: &[u8],
        clock: impl FnMut() -> u64,
    ) -> Result<()> {
        let mut control = LocalControl(clock);
        self.stage_content_controlled(manager, host, instance, request, bytes, &mut control)
    }

    /// Retain create/replacement bytes through the audited Core staging transaction.
    /// Selection, immutable plan, payload digest and current authority must all
    /// agree. This writes only the content library, never the selected OS file.
    /// Capacity is shared with the instance's other IO jobs. The immutable
    /// payload length is charged on each call; codec/index overhead is separately
    /// accounted by the Store. Errors after commit do not imply missing history.
    pub fn stage_content_controlled(
        &self,
        manager: &Manager,
        host: &mut HostRuntime,
        instance: &ManagedInstance,
        request: &RequestRecord,
        bytes: &[u8],
        control: &mut impl TargetControl,
    ) -> Result<()> {
        self.validate_request_controlled(manager, host, instance, request, control)?;
        let value = request.request();
        if !matches!(
            value.disposition,
            Disposition::Create | Disposition::Replace
        ) || value.content_length != bytes.len() as u64
        {
            return Err(Error::Mismatch);
        }
        if bytes.len() > MAX_CONTENT_BYTES {
            return Err(Error::Limit);
        }
        let lease = self.target_lease(request)?;
        let binding = lease.binding();
        let _job = controlled(control, |now| {
            Ok(binding.admit(
                manager,
                host,
                instance,
                value.disposition.capability(),
                0,
                bytes.len() as u64,
                now,
            )?)
        })?;
        let content = FileContent::new(
            &value.operation_id,
            &value.subject,
            request.command()?.request_sha256,
            bytes,
        )?;
        if Some(content.content_sha256()) != value.content_sha256 {
            return Err(Error::Mismatch);
        }
        let mut rejection = None;
        let result = host
            .store_local_mut()
            .stage_file_mutation_content_local_authorized(&content, || {
                controlled(control, |now| {
                    binding.check_liveness(now)?;
                    Ok(())
                })
                .map_err(|error| {
                    rejection = Some(error);
                    morrow_core::Error::Invalid("inactive selected file staging")
                })
            });
        result.map_err(|error| rejection.unwrap_or(Error::Persistence(error)))?;
        delivery(controlled(control, |now| {
            lease.check(manager, host, instance, now)?;
            Ok(())
        }))?;
        Ok(())
    }
}
