//! One-shot directory-bound create: committed claim, staged write, no-overwrite publish.
use super::*;
use morrow_core::{
    file_effect::{CreateOutcome, CreateResult},
    io_evidence::{Kind, Material},
    io_intent::{ObservationSource, Phase},
};
impl TargetBroker {
    /// Creates a new leaf under the retained parent. The destination is never
    /// overwritten. Temporary files are external effects too and only start
    /// after a confirmed Unknown claim. Crash leftovers require reconciliation;
    /// neither the operation nor temporary cleanup is automatically replayed.
    pub fn create(
        &mut self,
        manager: &Manager,
        host: &mut HostRuntime,
        instance: &ManagedInstance,
        request: &RequestRecord,
        clock: impl FnMut() -> u64,
    ) -> Result<CreateOutcome> {
        self.create_controlled(manager, host, instance, request, &mut LocalControl(clock))
    }
    /// Cancellable host adapter. Clock sampling and validation run within the
    /// supplied control; no lock spans filesystem IO or a Store transaction.
    pub fn create_controlled(
        &mut self,
        manager: &Manager,
        host: &mut HostRuntime,
        instance: &ManagedInstance,
        request: &RequestRecord,
        control: &mut impl TargetControl,
    ) -> Result<CreateOutcome> {
        let command = request.command()?;
        let value = request.request();
        let reference = value.target.reference;
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
        if value.disposition != Disposition::Create {
            return Err(Error::Mismatch);
        }
        if !native_windows::available() {
            return Err(Error::RestartRequired);
        }
        let leaf = value
            .target
            .relative_path
            .as_ref()
            .ok_or(Error::Mismatch)?
            .as_str()
            .rsplit('/')
            .next()
            .ok_or(Error::Mismatch)?;
        let temporary = format!(
            ".morrow-create-{}.tmp",
            command
                .request_sha256
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect::<String>()
        );
        if temporary == leaf {
            return Err(Error::Mismatch);
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
        let gate = native_windows::enter().map_err(|error| match error {
            native_windows::GateError::Busy => Error::Busy,
            native_windows::GateError::RestartRequired => Error::RestartRequired,
        })?;
        let entry = self.creates.get(&reference).ok_or(Error::Missing)?;
        let binding = entry.lease().delivery_guard();
        // Temporary file capacity is additional to ALL retained directories.
        let temporary_lease = controlled(control, |now| {
            Ok(binding.admit_resource(
                manager,
                host,
                instance,
                &[Disposition::Create.capability()],
                now,
            )?)
        })?;
        // Count payload AND command/result bytes, including repeated attempts
        // that fail after admission. Codec maximum is not an execution quota.
        let charge = command
            .request_bytes
            .checked_add(command.response_limit)
            .and_then(|value| value.checked_add(request.request().content_length))
            .ok_or(Error::Limit)?;
        let _job = controlled(control, |now| {
            Ok(binding.admit(
                manager,
                host,
                instance,
                Disposition::Create.capability(),
                0,
                charge,
                now,
            )?)
        })?;
        let mut rejection = None;
        let content = host
            .store_local()
            .file_mutation_content_local_authorized(&command.subject, &command.operation_id, || {
                controlled(control, |now| Ok(binding.check_liveness(now)?)).map_err(|error| {
                    rejection = Some(error);
                    morrow_core::Error::Invalid("inactive file create content access")
                })
            })
            .map_err(|error| rejection.take().unwrap_or(Error::Persistence(error)))?
            .ok_or(Error::Persistence(morrow_core::Error::EvidenceUnavailable))?;
        if content.request_sha256() != command.request_sha256
            || content.content().len() as u64 != value.content_length
            || Some(content.content_sha256()) != value.content_sha256
        {
            return Err(Error::Mismatch);
        }
        // The Core claim independently rechecks these exact retained originals
        // and the receipt in its own transaction. Local bytes never substitute
        // for that durable evidence; this slice is used only after claim success.
        let candidate = stored.propose_dispatch_boundary()?;
        let claim = host
            .store_local_mut()
            .claim_file_create_local_authorized(&candidate, || {
                controlled(control, |now| Ok(binding.check_liveness(now)?)).map_err(|error| {
                    rejection = Some(error);
                    morrow_core::Error::Invalid("inactive selected file create")
                })
            });
        let claimed = match claim {
            Ok(record) => record,
            Err(morrow_core::Error::CommitUnknown) => {
                self.creates.remove(&reference);
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
        let entry = self
            .creates
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
        let parent = entry.handles.last().ok_or(Error::OutcomeUnknown)?;
        let result = match gate.create(parent, &temporary, leaf, content.content(), || {
            controlled(control, |now| {
                Ok(binding.check(manager, host, instance, now)?)
            })
            .is_ok()
        }) {
            native_windows::CreateAttempt::Created => CreateResult::Created,
            native_windows::CreateAttempt::Rejected(code) => CreateResult::OsRejected { code },
            native_windows::CreateAttempt::Unknown { close_unknown } => {
                if close_unknown {
                    std::mem::forget(temporary_lease);
                }
                return Err(Error::OutcomeUnknown);
            }
        };
        fault("after-effect");
        // Record the actual OS outcome even after revocation; only its delivery
        // remains conditional on original live authority.
        control.with(|_, _| ());
        let outcome = CreateOutcome::new(
            &command.operation_id,
            &command.subject,
            command.request_sha256,
            reference,
            content.content_sha256(),
            value.content_length,
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
        host.store_local_mut()
            .observe_file_create_local_authorized(&observed, &material, || Ok(()))
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
    if std::env::var("MORROW_FILE_CREATE_FAULT").ok().as_deref() == Some(_point) {
        std::process::exit(86);
    }
}
