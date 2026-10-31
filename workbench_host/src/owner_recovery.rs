//! Resource-only recovery. This is never normal shutdown or business completion.
use super::*;
use crate::supervision_wire::{Preview, Recovery};
use codec::proto::{PreviewBinding, RecoveryBinding, RecoveryHistoryBinding, RecoveryReceipt};
use morrow_native_pipe_win::job::ProcessWatch;
use prost::Message;
use rusqlite::OpenFlags;
pub(super) fn complete_abnormal_proof(r: &OwnerRecord) -> bool {
    codec::complete_abnormal_proof(r)
}
pub(super) fn create_history(db: &Connection) -> Result<()> {
    db.execute_batch("CREATE TABLE IF NOT EXISTS owner_recovery(sequence INTEGER PRIMARY KEY,original_record BLOB NOT NULL,original_digest BLOB NOT NULL,receipt BLOB NOT NULL,receipt_digest BLOB NOT NULL,previous_head BLOB NOT NULL,head BLOB NOT NULL);")?;
    history_head(db)?;
    Ok(())
}
pub(super) fn head(db: &Connection) -> Result<Vec<u8>> {
    history_head(db)
}
fn chain(
    sequence: i64,
    original: &[u8],
    digest: &[u8],
    receipt: &[u8],
    receipt_digest: &[u8],
    prior: &[u8],
) -> Result<Vec<u8>> {
    if sequence <= 0 {
        return Err("Recovery sequence invalid".into());
    }
    Ok(sha_bytes(
        &RecoveryHistoryBinding {
            version: 1,
            sequence: sequence as u64,
            original_record_container: original.to_vec(),
            original_digest: digest.to_vec(),
            receipt_container: receipt.to_vec(),
            receipt_digest: receipt_digest.to_vec(),
            previous_head: prior.to_vec(),
        }
        .encode_to_vec(),
    ))
}
fn history_head(db: &Connection) -> Result<Vec<u8>> {
    let present: i64 = db.query_row(
        "SELECT count(*) FROM sqlite_master WHERE type='table' AND name='owner_recovery'",
        [],
        |r| r.get(0),
    )?;
    if present == 0 {
        return Ok(Vec::new());
    }
    let mut stmt=db.prepare("SELECT sequence,original_record,original_digest,receipt,receipt_digest,previous_head,head FROM owner_recovery ORDER BY sequence")?;
    let mut rows = stmt.query([])?;
    let mut previous = Vec::new();
    let mut sequence = 0i64;
    while let Some(row) = rows.next()? {
        sequence = sequence
            .checked_add(1)
            .ok_or("recovery history exhausted")?;
        let actual: i64 = row.get(0)?;
        let original: Vec<u8> = row.get(1)?;
        let digest: Vec<u8> = row.get(2)?;
        let receipt: Vec<u8> = row.get(3)?;
        let receipt_digest: Vec<u8> = row.get(4)?;
        let prior: Vec<u8> = row.get(5)?;
        let head: Vec<u8> = row.get(6)?;
        let r = decode(original.clone(), digest.clone())?;
        let id = codec::identity(&r)?;
        let proof = codec::decode_receipt(&receipt)?;
        if actual != sequence
            || prior != previous
            || head
                != chain(
                    actual,
                    &original,
                    &digest,
                    &receipt,
                    &receipt_digest,
                    &prior,
                )?
            || r.phase != OwnerPhase::Closing as i32
            || id.recovery_history_head != prior
            || !complete_abnormal_proof(&r)
            || receipt_digest != sha_bytes(&receipt)
            || proof.original_digest != digest
            || proof.profile != id.profile
            || proof.generation != id.generation
            || proof.incarnation != id.incarnation
            || proof.manager_sha256 != id.supervisor_sha256
        {
            return Err("Recovery history integrity or original resource proof is invalid".into());
        }
        previous = head;
    }
    Ok(previous)
}
pub(super) fn validate_history_binding(db: &Connection, r: &OwnerRecord) -> Result<()> {
    let present: i64 = db.query_row(
        "SELECT count(*) FROM sqlite_master WHERE type='table' AND name='owner_recovery'",
        [],
        |row| row.get(0),
    )?;
    if present != 1 {
        return Err("Recovery history table missing; no automatic repair".into());
    }
    if r.phase != OwnerPhase::Recovered as i32
        && codec::identity(r)?.recovery_history_head != history_head(db)?
    {
        return Err("Owner recovery history head missing or changed".into());
    }
    Ok(())
}
pub(super) fn validate_recovered(db: &Connection, r: &OwnerRecord) -> Result<()> {
    validate(r)?;
    if r.phase != OwnerPhase::Recovered as i32 {
        return Err("Expected resource-only Recovered owner".into());
    }
    let current_head = history_head(db)?;
    let binding = r.recovery.as_ref().ok_or("Recovery binding missing")?;
    let (original,digest,receipt_digest,head):(Vec<u8>,Vec<u8>,Vec<u8>,Vec<u8>)=db.query_row(
        "SELECT original_record,original_digest,receipt_digest,head FROM owner_recovery WHERE head=?1",[&binding.history_head],
        |row|Ok((row.get(0)?,row.get(1)?,row.get(2)?,row.get(3)?)))?;
    let mut expected = decode(original, digest.clone())?;
    expected.phase = OwnerPhase::Recovered as i32;
    expected.recovery = Some(RecoveryBinding {
        version: 1,
        original_digest: digest,
        receipt_digest,
        history_head: head.clone(),
    });
    if *r != expected || current_head != head {
        return Err("Recovered owner differs from archived original proof".into());
    }
    Ok(())
}
fn open(root: &Path, writable: bool) -> Result<Connection> {
    reject_legacy(root)?;
    if std::fs::read(root.join(MARKER_NAME))? != MARKER {
        return Err("Supervisor marker missing or changed; preserve evidence".into());
    }
    let db = Connection::open_with_flags(
        root.join("workbench-supervisor.sqlite"),
        if writable {
            OpenFlags::SQLITE_OPEN_READ_WRITE
        } else {
            OpenFlags::SQLITE_OPEN_READ_ONLY
        },
    )?;
    db.busy_timeout(std::time::Duration::from_millis(100))?;
    if writable {
        db.execute_batch("PRAGMA synchronous=FULL;")?;
    }
    schema(&db)?;
    Ok(db)
}
fn read(db: &Connection) -> Result<(Vec<u8>, Vec<u8>, OwnerRecord)> {
    let (bytes, digest) = read_row(db)?;
    let r = decode(bytes.clone(), digest.clone())?;
    Ok((bytes, digest, r))
}
/// Excludes a still-live writer; does not supply any resource proof.
fn original_ended(r: &OwnerRecord) -> Result<bool> {
    let id = codec::identity(r)?;
    match ProcessWatch::open(id.supervisor_pid) {
        Ok(p) => Ok(p.creation_filetime()? != id.supervisor_creation_filetime || p.is_exited()?),
        Err(e) if e.raw_os_error() == Some(87) => Ok(true),
        Err(e) => Err(e.into()),
    }
}
fn reason(r: &OwnerRecord, root: &Path, host: &Path, package: &Path) -> Result<&'static str> {
    let id = codec::identity(r)?;
    if id.profile != unhex(&profile(root))? {
        return Ok("profile_mismatch");
    }
    if r.phase == OwnerPhase::Released as i32 {
        return Ok("already_normal_released");
    }
    if r.phase != OwnerPhase::Recovered as i32
        && (r.phase != OwnerPhase::Closing as i32 || !complete_abnormal_proof(r))
    {
        return Ok("original_resource_proof_missing");
    }
    if id.host_sha256 != sha_bytes(&std::fs::read(host)?)
        || id.package_sha256 != sha_bytes(&std::fs::read(package)?)
        || id.supervisor_sha256 != sha_bytes(&std::fs::read(std::env::current_exe()?)?)
    {
        return Ok("artifact_binding_mismatch");
    }
    if r.phase == OwnerPhase::Recovered as i32 {
        return Ok("already_resource_recovered");
    }
    if !original_ended(r)? {
        return Ok("original_supervisor_still_live");
    }
    Ok("eligible_resource_only")
}
fn snapshot(db: &Connection, root: &Path, host: &Path, package: &Path) -> Result<Preview> {
    schema(db)?;
    let (bytes, digest, r) = read(db)?;
    let head = history_head(db)?;
    validate_history_binding(db, &r)?;
    if r.phase == OwnerPhase::Recovered as i32 {
        validate_recovered(db, &r)?;
    }
    let reason = reason(&r, root, host, package)?;
    let profile = unhex(&profile(root))?;
    let token = sha_bytes(
        &PreviewBinding {
            version: 1,
            sqlite_schema_version: SCHEMA_VERSION as u32,
            profile: profile.clone(),
            original_record_container: bytes,
            original_digest: digest.clone(),
            history_head: head.clone(),
            reason: reason.into(),
        }
        .encode_to_vec(),
    );
    Ok(Preview {
        profile: profile.try_into().map_err(|_| "profile")?,
        token: token.try_into().map_err(|_| "token")?,
        original_digest: digest.try_into().map_err(|_| "digest")?,
        history_head: head,
        eligible: reason == "eligible_resource_only",
        reason: reason.into(),
        resource_recovery_confirmed: reason == "already_resource_recovered",
        recovery_original_digest: if reason == "already_resource_recovered" {
            Some(
                r.recovery
                    .as_ref()
                    .ok_or("recovery")?
                    .original_digest
                    .clone()
                    .try_into()
                    .map_err(|_| "original digest")?,
            )
        } else {
            None
        },
        record: r,
    })
}
pub fn preview(root: &Path, host: &Path, package: &Path) -> Result<Preview> {
    let root = root.canonicalize()?;
    let mut db = open(&root, false)?;
    let tx = db.transaction()?;
    let result = snapshot(&tx, &root, host, package)?;
    tx.commit()?;
    Ok(result)
}
/// Explicit one-shot management; no guest started and no operation replayed.
pub fn recover(
    root: &Path,
    host: &Path,
    package: &Path,
    expected_token: &str,
    acknowledge_unknown: bool,
) -> Result<Recovery> {
    if !acknowledge_unknown {
        return Err("Explicit acknowledgement of retained Unknown is required".into());
    }
    let expected_token = unhex(expected_token)?;
    let root = root.canonicalize()?;
    let mut db = open(&root, true)?;
    let tx = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let preview = snapshot(&tx, &root, host, package)?;
    if preview.token.as_slice() != expected_token || !preview.eligible {
        return Err(format!(
            "Owner recovery refused: {}. Preserve original evidence.",
            preview.reason
        )
        .into());
    }
    let (bytes, digest, mut r) = read(&tx)?;
    let id = codec::identity(&r)?;
    let previous_head = history_head(&tx)?;
    let sequence: i64 = tx.query_row("SELECT count(*)+1 FROM owner_recovery", [], |r| r.get(0))?;
    let receipt = codec::encode_receipt(&RecoveryReceipt {
        version: 1,
        acknowledged_unknown: true,
        original_digest: digest.clone(),
        profile: id.profile.clone(),
        generation: id.generation,
        incarnation: id.incarnation.clone(),
        manager_pid: std::process::id(),
        manager_creation_filetime: ProcessWatch::open(std::process::id())?.creation_filetime()?,
        manager_sha256: sha_bytes(&std::fs::read(std::env::current_exe()?)?),
        original_supervisor_ended: true,
    })?;
    let receipt_digest = sha_bytes(&receipt);
    let head = chain(
        sequence,
        &bytes,
        &digest,
        &receipt,
        &receipt_digest,
        &previous_head,
    )?;
    tx.execute(
        "INSERT INTO owner_recovery VALUES(?1,?2,?3,?4,?5,?6,?7)",
        params![
            sequence,
            bytes,
            digest,
            receipt,
            receipt_digest,
            previous_head,
            head
        ],
    )?;
    r.phase = OwnerPhase::Recovered as i32;
    r.recovery = Some(RecoveryBinding {
        version: 1,
        original_digest: digest.clone(),
        receipt_digest,
        history_head: head.clone(),
    });
    validate_recovered(&tx, &r)?;
    let recovered = codec::encode_owner(&r)?;
    if tx.execute(
        "UPDATE owner SET record=?1,digest=?2 WHERE singleton=1 AND record=?3 AND digest=?4",
        params![recovered, sha_bytes(&recovered), bytes, digest],
    )? != 1
    {
        return Err("Owner recovery CAS refused".into());
    }
    readback(&tx, &recovered)?;
    schema(&tx)?;
    tx.commit()?;
    let (_, _, actual) = read(&db)?;
    validate_recovered(&db, &actual)?;
    if actual != r {
        return Err("Recovery commit result unconfirmed; preview again".into());
    }
    Ok(Recovery {
        profile: unhex(&profile(&root))?.try_into().map_err(|_| "profile")?,
        request_token: expected_token.try_into().map_err(|_| "token")?,
        original_digest: digest.try_into().map_err(|_| "digest")?,
        history_head: head.try_into().map_err(|_| "history")?,
        record: actual,
    })
}
#[cfg(test)]
#[path = "supervision_recovery_tests.rs"]
mod recovery_tests;
