//! Durable confirmation only; never a substitute for original kernel evidence.
use super::{Owner, Result, codec, decode, read_row, readback, recovery, schema, sha_bytes};
use codec::OwnerRecord;
use codec::proto::OwnerPhase;
pub(crate) struct AbnormalClosurePrepared {
    extension: crate::supervision_wire::PreparedAbnormal,
}
impl AbnormalClosurePrepared {
    pub(crate) fn into_extension(self) -> crate::supervision_wire::PreparedAbnormal {
        self.extension
    }
}
fn validate_abnormal_record(record: &OwnerRecord) -> Result<()> {
    codec::validate(record)?;
    if record.phase != OwnerPhase::Closing as i32
        || !codec::complete_abnormal_proof(record)
        || !record.reader_joins.as_ref().is_some_and(codec::joined)
    {
        return Err(
            "Original abnormal resource and reader-join proof incomplete; retain Unknown".into(),
        );
    }
    Ok(())
}
fn confirm_committed(owner: &Owner, expected: &OwnerRecord) -> Result<AbnormalClosurePrepared> {
    validate_abnormal_record(expected)?;
    let bytes = codec::encode_owner(expected)?;
    let tx = owner.db.unchecked_transaction()?;
    schema(&tx)?;
    readback(&tx, &bytes)?;
    recovery::validate_history_binding(&tx, expected)?;
    let (actual_bytes, digest) = read_row(&tx)?;
    let actual = decode(actual_bytes.clone(), digest.clone())?;
    if actual != *expected || actual_bytes != bytes || digest != sha_bytes(&bytes) {
        return Err("Abnormal commit readback unconfirmed; retain Unknown".into());
    }
    validate_abnormal_record(&actual)?;
    tx.commit()?;
    Ok(AbnormalClosurePrepared {
        extension: crate::supervision_wire::PreparedAbnormal {
            identity: codec::identity(&actual)?.clone(),
            committed_digest: digest.try_into().map_err(|_| "committed digest")?,
            resources: codec::facts(&actual)?.clone(),
            reader_joins: actual.reader_joins.ok_or("reader joins")?,
        },
    })
}
impl Owner {
    /// Only trusted original child/Job/EOF/reap/join observations may call this.
    pub(crate) fn prepare_abnormal_resource_closure(
        &mut self,
        record: OwnerRecord,
    ) -> Result<AbnormalClosurePrepared> {
        validate_abnormal_record(&record)?;
        self.update(record.clone())?;
        // A commit or readback error never retries, releases or creates a new owner.
        confirm_committed(self, &record)
    }
}
#[cfg(test)]
#[path = "supervision_quarantine_tests.rs"]
mod tests;
