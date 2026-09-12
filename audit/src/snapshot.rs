//! Versioned streaming PB/LZ4 backup. SQLite pages remain engine-owned payload.
use crate::{
    TrustedLog,
    keys::Key,
    session::{Result, SessionError, reject_link},
};
use morrow_core::store::Store;
use prost::Message;
use sha2::{Digest, Sha256};
use std::{
    fs::File,
    io::{Read, Seek, SeekFrom, Write},
    path::Path,
};
mod proto {
    include!(concat!(env!("OUT_DIR"), "/morrow.snapshot.v1.rs"));
}
const MAGIC: &[u8; 8] = b"MORROWS1";
const CHUNK: usize = 1024 * 1024;
const MAX_DB: u64 = 8 * 1024 * 1024 * 1024;
fn invalid() -> SessionError {
    SessionError::SnapshotFormat
}
fn frame(file: &mut File, value: &impl Message) -> Result<()> {
    let bytes = lz4_flex::block::compress_prepend_size(&value.encode_to_vec());
    file.write_all(&(bytes.len() as u32).to_le_bytes())?;
    file.write_all(&bytes)?;
    Ok(())
}
fn read_frame<M: Message + Default>(file: &mut File, limit: usize) -> Result<M> {
    let mut size = [0; 4];
    file.read_exact(&mut size)?;
    let size = u32::from_le_bytes(size) as usize;
    if size < 4 || size > limit + limit / 255 + 64 {
        return Err(invalid());
    }
    let mut packed = vec![0; size];
    file.read_exact(&mut packed)?;
    let raw = u32::from_le_bytes(packed[..4].try_into().unwrap()) as usize;
    if raw == 0 || raw > limit {
        return Err(invalid());
    }
    let bytes = lz4_flex::block::decompress_size_prepended(&packed).map_err(|_| invalid())?;
    M::decode(bytes.as_slice()).map_err(|_| invalid())
}
fn hash_file(file: &mut File) -> Result<Vec<u8>> {
    file.seek(SeekFrom::Start(0))?;
    let mut hash = Sha256::new();
    let mut bytes = vec![0; CHUNK];
    loop {
        let n = file.read(&mut bytes)?;
        if n == 0 {
            break;
        };
        hash.update(&bytes[..n]);
    }
    file.seek(SeekFrom::Start(0))?;
    Ok(hash.finalize().to_vec())
}
/// Check every bounded frame and the whole database digest, optionally extracting.
fn read_archive(file: &mut File, mut target: Option<&mut File>) -> Result<proto::Manifest> {
    if file.metadata()?.len() > MAX_DB + 64 * 1024 * 1024 {
        return Err(invalid());
    }
    file.seek(SeekFrom::Start(0))?;
    let mut magic = [0; 8];
    file.read_exact(&mut magic)?;
    if &magic != MAGIC {
        return Err(invalid());
    }
    let manifest: proto::Manifest = read_frame(file, 128 * 1024)?;
    if manifest.version != 1
        || manifest.database_bytes == 0
        || manifest.database_bytes > MAX_DB
        || manifest.database_sha256.len() != 32
        || manifest.chunk_bytes != CHUNK as u32
        || manifest.protected_key.len() > 65536
    {
        return Err(invalid());
    }
    let key = Key::from_protected(&manifest.protected_key)?;
    if key.trust().id != manifest.log_id
        || key.trust().key.as_bytes().as_slice() != manifest.public_key
    {
        return Err(invalid());
    }
    let mut remaining = manifest.database_bytes;
    let mut hash = Sha256::new();
    for index in 0..manifest.database_bytes.div_ceil(CHUNK as u64) {
        let chunk: proto::Chunk = read_frame(file, CHUNK + 64)?;
        let expected = remaining.min(CHUNK as u64) as usize;
        if chunk.index as u64 != index || chunk.payload.len() != expected {
            return Err(invalid());
        }
        if let Some(target) = target.as_deref_mut() {
            target.write_all(&chunk.payload)?;
        }
        hash.update(&chunk.payload);
        remaining -= expected as u64;
    }
    let mut tail = [0; 1];
    if file.read(&mut tail)? != 0 || hash.finalize().as_slice() != manifest.database_sha256 {
        return Err(invalid());
    }
    Ok(manifest)
}
pub(crate) fn write(
    store: &Store,
    source_key: &Path,
    destination: &Path,
    trust: &TrustedLog,
) -> Result<()> {
    reject_link(source_key)?;
    reject_link(destination)?;
    if destination.try_exists()? {
        return Err(SessionError::BackupAlreadyExists);
    }
    let (key, protected_key) = Key::validated_copy(source_key)?;
    if key.trust().id != trust.id || key.trust().key != trust.key {
        return Err(SessionError::KeyMismatch);
    }
    let parent = destination
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let staging = tempfile::tempdir_in(parent)?;
    let database = staging.path().join("workbench.db");
    store.snapshot_to(&database, MAX_DB)?;
    drop(Store::open_read_only_audited(&database, trust.clone())?);
    let mut source = File::options().read(true).write(true).open(&database)?;
    source.sync_all()?;
    let database_bytes = source.metadata()?.len();
    let manifest = proto::Manifest {
        version: 1,
        database_bytes,
        database_sha256: hash_file(&mut source)?,
        chunk_bytes: CHUNK as u32,
        protected_key,
        log_id: trust.id.clone(),
        public_key: trust.key.as_bytes().to_vec(),
    };
    let mut output = tempfile::NamedTempFile::new_in(parent)?;
    output.write_all(MAGIC)?;
    frame(output.as_file_mut(), &manifest)?;
    let mut remaining = database_bytes;
    for index in 0..database_bytes.div_ceil(CHUNK as u64) {
        let mut bytes = vec![0; remaining.min(CHUNK as u64) as usize];
        source.read_exact(&mut bytes)?;
        remaining -= bytes.len() as u64;
        frame(
            output.as_file_mut(),
            &proto::Chunk {
                index: index as u32,
                payload: bytes,
            },
        )?;
    }
    output.as_file().sync_all()?;
    if read_archive(output.as_file_mut(), None)? != manifest {
        return Err(invalid());
    }
    boundary("snapshot-before-publish");
    let published = output.persist_noclobber(destination).map_err(|e| {
        if e.error.kind() == std::io::ErrorKind::AlreadyExists {
            SessionError::BackupAlreadyExists
        } else {
            SessionError::BackupPublishUnknown
        }
    })?;
    boundary("snapshot-after-publish");
    published
        .sync_all()
        .map_err(|_| SessionError::BackupPublishUnknown)?;
    Ok(())
}
/// Restore into a new directory only, never onto a live or existing content library.
pub fn restore(archive: &Path, destination: &Path) -> Result<()> {
    reject_link(archive)?;
    reject_link(destination)?;
    if destination.try_exists()? {
        return Err(SessionError::SnapshotDestinationExists);
    }
    let parent = destination
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let staging = tempfile::tempdir_in(parent)?;
    let mut input = File::open(archive)?;
    if !input.metadata()?.is_file() {
        return Err(invalid());
    }
    let database = staging.path().join("workbench.db");
    let mut output = File::options()
        .write(true)
        .create_new(true)
        .open(&database)?;
    let manifest = read_archive(&mut input, Some(&mut output))?;
    output.sync_all()?;
    drop(output);
    let key = Key::from_protected(&manifest.protected_key)?;
    drop(Store::open_read_only_audited(&database, key.trust())?);
    let mut protected = File::options()
        .write(true)
        .create_new(true)
        .open(staging.path().join("workbench.db.audit-key"))?;
    protected.write_all(&manifest.protected_key)?;
    protected.sync_all()?;
    drop(protected);
    boundary("snapshot-restore-before-publish");
    // Windows directory rename refuses an existing destination, even an empty one.
    std::fs::rename(staging.path(), destination)
        .map_err(|_| SessionError::SnapshotPublishUnknown)?;
    let _ = staging.keep();
    boundary("snapshot-restore-after-publish");
    Ok(())
}
fn boundary(_name: &str) {
    #[cfg(feature = "fault-injection")]
    if std::env::var("MORROW_AUDIT_CRASH_AT").as_deref() == Ok(_name) {
        std::process::exit(86);
    }
}
