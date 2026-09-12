//! Export a fixed protected container; no raw secret export and no overwrite.
use crate::{
    TrustedLog,
    keys::Key,
    session::{Result, SessionError, reject_link},
};
use std::{io::Write, path::Path};
pub(crate) fn write(source: &Path, destination: &Path, trust: &TrustedLog) -> Result<()> {
    reject_link(source)?;
    reject_link(destination)?;
    if destination.try_exists()? {
        return Err(SessionError::BackupAlreadyExists);
    }
    let (key, bytes) = Key::validated_copy(source)?;
    if key.trust().id != trust.id || key.trust().key != trust.key {
        return Err(SessionError::KeyMismatch);
    }
    let parent = destination
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let mut staged = tempfile::NamedTempFile::new_in(parent)?;
    staged.write_all(&bytes)?;
    staged.as_file().sync_all()?;
    boundary("backup-before-publish");
    let published = staged.persist_noclobber(destination).map_err(|e| {
        if e.error.kind() == std::io::ErrorKind::AlreadyExists {
            SessionError::BackupAlreadyExists
        } else {
            SessionError::BackupPublishUnknown
        }
    })?;
    boundary("backup-after-publish");
    published
        .sync_all()
        .map_err(|_| SessionError::BackupPublishUnknown)?;
    drop(published);
    let (actual, saved) =
        Key::validated_copy(destination).map_err(|_| SessionError::BackupPublishUnknown)?;
    if saved != bytes || actual.trust().id != trust.id || actual.trust().key != trust.key {
        return Err(SessionError::BackupPublishUnknown);
    }
    Ok(())
}
fn boundary(_name: &str) {
    #[cfg(feature = "fault-injection")]
    if std::env::var("MORROW_AUDIT_CRASH_AT").as_deref() == Ok(_name) {
        std::process::exit(86);
    }
}
