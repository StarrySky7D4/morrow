//! Restore only an existing identity, under the same cooperative lease as the host.
use crate::{
    keys::{self, Key},
    session::{self, Result, SessionError},
};
use morrow_core::store::{AuditBindingState, Store};
use std::{
    io::Write,
    path::{Path, PathBuf},
};
#[derive(Debug, PartialEq, Eq)]
pub enum Recovery {
    AlreadyPresent,
    Restored { preserved: Option<PathBuf> },
}
/// Never migrates or writes the database. Selected bytes are fixed before verification.
pub fn restore_key(database: &Path, selected: &Path) -> Result<Recovery> {
    let (database, _lease) = session::locked_database(database)?;
    session::reject_link(selected)?;
    let status = Store::audit_binding_status(&database)?;
    if !matches!(
        status,
        AuditBindingState::Bound(_) | AuditBindingState::LegacySealed
    ) {
        return Err(SessionError::RecoveryRequiresBinding);
    }
    let (key, bytes) = Key::validated_copy(selected)?;
    let trust = key.trust();
    if let AuditBindingState::Bound(binding) = &status
        && (binding.log_id != trust.id || binding.public_key != *trust.key.as_bytes())
    {
        return Err(SessionError::KeyMismatch);
    }
    // A matching header alone is insufficient; verify the complete original history.
    let verified_store = Store::open_read_only_audited(&database, trust.clone())?;
    let destination = session::key_path(&database)?;
    let parent = database.parent().ok_or(SessionError::InvalidPath)?;
    let existing = destination.try_exists()?;
    let preserved = if existing {
        if Key::load(&destination)
            .is_ok_and(|old| old.trust().id == trust.id && old.trust().key == trust.key)
        {
            return Ok(Recovery::AlreadyPresent);
        }
        let old = keys::read_protected(&destination)?;
        let mut backup = tempfile::Builder::new()
            .prefix("morrow-audit-key-before-restore-")
            .suffix(".backup")
            .tempfile_in(parent)?;
        backup.write_all(&old)?;
        backup.as_file().sync_all()?;
        let (file, path) = backup.keep().map_err(|e| SessionError::Io(e.error))?;
        file.sync_all()?;
        Some(path)
    } else {
        None
    };
    boundary("restore-after-backup");
    let mut staged = tempfile::NamedTempFile::new_in(parent)?;
    staged.write_all(&bytes)?;
    staged.as_file().sync_all()?;
    boundary("restore-before-publish");
    // Existing damaged bytes are durably retained before atomic replacement.
    // For a missing destination, do not overwrite a file that appeared meanwhile.
    let published = if existing {
        staged.persist(&destination)
    } else {
        staged.persist_noclobber(&destination)
    }
    .map_err(|_| SessionError::RecoveryPublishUnknown)?;
    boundary("restore-after-publish");
    published
        .sync_all()
        .map_err(|_| SessionError::RecoveryPublishUnknown)?;
    drop(published);
    let actual = Key::load(&destination).map_err(|_| SessionError::RecoveryPublishUnknown)?;
    if actual.trust().id != trust.id || actual.trust().key != trust.key {
        return Err(SessionError::RecoveryPublishUnknown);
    }
    drop(verified_store);
    Ok(Recovery::Restored { preserved })
}
fn boundary(_name: &str) {
    #[cfg(feature = "fault-injection")]
    if std::env::var("MORROW_AUDIT_CRASH_AT").as_deref() == Ok(_name) {
        std::process::exit(86);
    }
}
