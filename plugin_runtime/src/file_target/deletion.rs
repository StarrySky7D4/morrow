//! One-shot selected-file deletion with a durable before-effect claim.
use super::*;
use morrow_core::{
    file_effect::{DeleteOutcome, DeleteResult},
    io_evidence::{Kind, Material},
    io_intent::{ObservationSource, Phase},
};
impl TargetBroker {
    /// Deletes the selected directory entry via its retained handle. Other hard
    /// links are not removed. Windows confirmation is not a power-loss guarantee.
    /// A durable Unknown is committed BEFORE setting a deletion disposition.
    /// Every failure after that boundary requires reconciliation, never replay.
    /// The selected target is consumed once; successful/failed API observations
    /// are retained even if delivery authority was revoked after the effect.
    pub fn delete(
        &mut self,
        manager: &Manager,
        host: &mut HostRuntime,
        instance: &ManagedInstance,
        request: &RequestRecord,
        clock: impl FnMut() -> u64,
    ) -> Result<DeleteOutcome> {
        self.delete_controlled(manager, host, instance, request, &mut LocalControl(clock))
    }
    /// Cancellable host adapter. Clock sampling and validation run within the
    /// supplied control; no lock spans filesystem IO or a Store transaction.
    pub fn delete_controlled(
        &mut self,
        manager: &Manager,
        host: &mut HostRuntime,
        instance: &ManagedInstance,
        request: &RequestRecord,
        control: &mut impl TargetControl,
    ) -> Result<DeleteOutcome> {
        let command = request.command()?;
        let reference = request.request().target.reference;
        if let Some(spent) = self.spent.get(&reference) {
            // Cancellation of this delivery cannot erase a previous dispatch.
            // Verify the original owner before exposing the spent history.
            control.with(|now, _| spent.binding.check(manager, host, instance, now))?;
            if spent.request_sha256 != command.request_sha256 {
                return Err(Error::Mismatch);
            }
            return Err(if spent.claimed {
                Error::AlreadyDispatched
            } else {
                Error::OutcomeUnknown
            });
        }
        self.validate_request_controlled(manager, host, instance, request, control)?;
        if request.request().disposition != Disposition::Delete {
            return Err(Error::Mismatch);
        }
        if !native_windows::available() {
            return Err(Error::RestartRequired);
        }
        let stored = host
            .store_local()
            .lookup_io_intent(&command.subject, &command.operation_id)?
            .ok_or(Error::Persistence(morrow_core::Error::NotFound))?;
        stored.matches_command(&command)?;
        if matches!(stored.phase(), Phase::OutcomeUnknown | Phase::Observed) {
            return Err(Error::AlreadyDispatched);
        }
        if stored.phase() != Phase::Prepared {
            return Err(Error::Mismatch);
        }
        let effect_gate = native_windows::enter().map_err(|error| match error {
            native_windows::GateError::Busy => Error::Busy,
            native_windows::GateError::RestartRequired => Error::RestartRequired,
        })?;
        let entry = self.entries.get(&reference).ok_or(Error::Missing)?;
        let binding = entry.lease.delivery_guard();
        let charge = command
            .request_bytes
            .checked_add(command.response_limit)
            .ok_or(Error::Limit)?;
        let _job = controlled(control, |now| {
            Ok(binding.admit(
                manager,
                host,
                instance,
                request.request().disposition.capability(),
                0,
                charge,
                now,
            )?)
        })?;
        let boundary = stored.propose_dispatch_boundary()?;
        let mut rejection = None;
        let claim = host
            .store_local_mut()
            .claim_file_delete_local_authorized(&boundary, || {
                controlled(control, |now| Ok(binding.check_liveness(now)?)).map_err(|error| {
                    rejection = Some(error);
                    morrow_core::Error::Invalid("inactive selected file delete")
                })
            });
        let claimed = match claim {
            Ok(record) => record,
            Err(morrow_core::Error::CommitUnknown) => {
                self.entries.remove(&reference);
                self.spent.insert(
                    reference,
                    Spent {
                        binding,
                        request_sha256: command.request_sha256,
                        claimed: false,
                    },
                );
                return Err(Error::Persistence(morrow_core::Error::CommitUnknown));
            }
            Err(error) => return Err(rejection.unwrap_or(Error::Persistence(error))),
        };
        // Retire BEFORE any post-claim callback or native call. A panic cannot
        // leave a reusable selection, and Drop cannot itself mark for deletion.
        let entry = self
            .entries
            .remove(&reference)
            .ok_or(Error::OutcomeUnknown)?;
        self.spent.insert(
            reference,
            Spent {
                binding: binding.duplicate(),
                request_sha256: command.request_sha256,
                claimed: true,
            },
        );
        fault("after-claim");
        let authorized = (|| -> Result<()> {
            controlled(control, |now| {
                Ok(entry.lease.check(manager, host, instance, now)?)
            })?;
            if Stamp::read(entry.file.metadata()?)? != entry.stamp {
                return Err(Error::Changed);
            }
            if !native_windows::available() {
                return Err(Error::RestartRequired);
            }
            controlled(control, |now| {
                Ok(entry.lease.check(manager, host, instance, now)?)
            })?;
            Ok(())
        })();
        if authorized.is_err() {
            return Err(Error::OutcomeUnknown);
        }
        let result = match effect_gate.delete(entry.file) {
            native_windows::DeleteAttempt::Deleted => DeleteResult::Deleted,
            native_windows::DeleteAttempt::Rejected(code) if code != 0 => {
                DeleteResult::OsRejected { code }
            }
            native_windows::DeleteAttempt::Rejected(_) => return Err(Error::OutcomeUnknown),
            native_windows::DeleteAttempt::CloseUnknown(_code) => {
                // Native handle validity is unknown. Do not pretend its resource
                // was freed. Admission is poisoned process-wide; keep this charge
                // until process teardown without ever re-closing a numeric handle.
                std::mem::forget(entry.lease);
                return Err(Error::OutcomeUnknown);
            }
        };
        fault("after-effect");
        // The host clock may revoke authority here. Retain the observed fact
        // regardless; only its final delivery is conditioned on current authority.
        control.with(|_, _| ());
        let outcome = DeleteOutcome::new(
            &command.operation_id,
            &command.subject,
            command.request_sha256,
            reference,
            request
                .request()
                .expected_identity
                .ok_or(Error::OutcomeUnknown)?,
            result,
        )
        .map_err(|_| Error::OutcomeUnknown)?;
        let material = Material::encode(
            Kind::Response,
            &command.operation_id,
            &command.subject,
            command.request_sha256,
            outcome.container(),
        )
        .map_err(|_| Error::OutcomeUnknown)?;
        let observed = claimed
            .propose_observation(material.digest(), ObservationSource::OriginalResponse)
            .map_err(|_| Error::OutcomeUnknown)?;
        // Original host and immutable observation are still held by this stack;
        // this records historical facts, not new authority to perform an effect.
        host.store_local_mut()
            .observe_file_delete_local_authorized(&observed, &material, || Ok(()))
            .map_err(|_| Error::OutcomeUnknown)?;
        fault("after-observe");
        delivery(controlled(control, |now| {
            Ok(binding.check(manager, host, instance, now)?)
        }))?;
        Ok(outcome)
    }
}
fn fault(_point: &str) {
    #[cfg(feature = "fault-injection")]
    if std::env::var("MORROW_FILE_DELETE_FAULT").ok().as_deref() == Some(_point) {
        std::process::exit(86);
    }
}
