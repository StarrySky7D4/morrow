//! Conditional replacement through the original selected native transaction.
use super::*;
use morrow_core::{
    file_effect::ReplaceOutcome,
    io_evidence::{Kind, Material},
    io_intent::{ObservationSource, Phase},
};

impl TargetBroker {
    pub fn replace(
        &mut self,
        manager: &Manager,
        host: &mut HostRuntime,
        instance: &ManagedInstance,
        request: &RequestRecord,
        clock: impl FnMut() -> u64,
    ) -> Result<ReplaceOutcome> {
        self.replace_controlled(manager, host, instance, request, &mut LocalControl(clock))
    }
    pub fn replace_controlled(
        &mut self,
        manager: &Manager,
        host: &mut HostRuntime,
        instance: &ManagedInstance,
        request: &RequestRecord,
        control: &mut impl TargetControl,
    ) -> Result<ReplaceOutcome> {
        let command = request.command()?;
        let value = request.request();
        let reference = value.target.reference;
        if let Some(spent) = self.spent.get(&reference) {
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
        if value.disposition != Disposition::Replace {
            return Err(Error::Mismatch);
        }
        let entry = self.entries.get(&reference).ok_or(Error::Missing)?;
        if !matches!(entry.file, TargetFile::Conditional(_)) {
            return Err(Error::UnsupportedConditionalReplacement);
        }
        let binding = entry.file.lease().delivery_guard();
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
        let charge = command
            .request_bytes
            .checked_add(command.response_limit)
            .and_then(|bytes| bytes.checked_add(value.content_length))
            .ok_or(Error::Limit)?;
        let job = controlled(control, |now| {
            Ok(binding.admit(
                manager,
                host,
                instance,
                Disposition::Replace.capability(),
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
                    morrow_core::Error::Invalid("inactive file replace content access")
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
        let boundary = stored.propose_dispatch_boundary()?;
        let claim = host
            .store_local_mut()
            .claim_file_replace_local_authorized(&boundary, || {
                controlled(control, |now| Ok(binding.check_liveness(now)?)).map_err(|error| {
                    rejection = Some(error);
                    morrow_core::Error::Invalid("inactive selected file replace")
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
        // Consume before any post-claim callback/effect. Cancellation and panic
        // cannot revive a selection or turn durable Unknown back into Prepared.
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
                Ok(binding.check(manager, host, instance, now)?)
            })?;
            if Stamp::read(entry.file.metadata()?)? != entry.stamp {
                return Err(Error::Changed);
            }
            Ok(())
        })();
        if authorized.is_err() {
            return Err(Error::OutcomeUnknown);
        }
        let TargetFile::Conditional(owner) = entry.file else {
            return Err(Error::OutcomeUnknown);
        };
        let temporary = format!(
            ".morrow-replace-{}.tmp",
            command
                .request_sha256
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect::<String>()
        );
        let result = owner.publish(&temporary, content.content(), job, || {
            controlled(control, |now| {
                Ok(binding.check(manager, host, instance, now)?)
            })
            .is_ok()
        })?;
        drop(gate);
        fault("after-effect");
        // Persist the actual native result even after revocation. This records
        // historical evidence and grants neither a new claim nor a native retry.
        control.with(|_, _| ());
        let outcome = ReplaceOutcome::new(
            &command.operation_id,
            &command.subject,
            command.request_sha256,
            reference,
            value.expected_identity.ok_or(Error::OutcomeUnknown)?,
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
            .observe_file_replace_local_authorized(&observed, &material, || Ok(()))
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
    if std::env::var("MORROW_FILE_REPLACE_FAULT").ok().as_deref() == Some(_point) {
        std::process::exit(86);
    }
}
