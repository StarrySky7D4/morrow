//! Windows product owner ledger. Unknown owners are never migrated or acquired.
use crate::{Result, supervision_record_codec as codec};
use codec::proto::{BusinessOutcome, OwnerPhase, ProtocolAck};
use codec::{Identity, OwnerRecord, ResourceFacts};
use rusqlite::{Connection, TransactionBehavior, params};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
pub struct Owner {
    db: Connection,
    record: OwnerRecord,
    encoded: Vec<u8>,
}
#[path = "owner_recovery.rs"]
pub mod recovery;
#[path = "startup_quarantine.rs"]
mod startup_quarantine;
const MARKER: &[u8] = b"Morrow supervised workbench v2 Protobuf LZ4\n";
const MARKER_NAME: &str = ".morrow-supervision-v2";
const LEGACY_MARKER_NAME: &str = ".morrow-supervision-v1";
const SCHEMA_VERSION: i64 = 2;
fn schema(db: &Connection) -> Result<()> {
    let version: i64 = db.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    let triggers: i64 = db.query_row(
        "SELECT count(*) FROM sqlite_master WHERE type='trigger'",
        [],
        |r| r.get(0),
    )?;
    if version != SCHEMA_VERSION || triggers != 0 {
        return Err("Supervisor binary owner schema changed or legacy; retain original evidence without automatic migration".into());
    }
    Ok(())
}
fn read_row(db: &Connection) -> Result<(Vec<u8>, Vec<u8>)> {
    Ok(db.query_row(
        "SELECT record,digest FROM owner WHERE singleton=1",
        [],
        |r| Ok((r.get::<_, Vec<u8>>(0)?, r.get::<_, Vec<u8>>(1)?)),
    )?)
}
fn readback(db: &Connection, expected: &[u8]) -> Result<()> {
    let (bytes, digest) = read_row(db)?;
    if bytes != expected || digest != sha_bytes(expected) {
        return Err("Supervisor durable owner write verification failed".into());
    }
    decode(bytes, digest)?;
    Ok(())
}
pub fn profile(root: &Path) -> String {
    let text = root.to_string_lossy().replace('\\', "/");
    let text = if let Some(v) = text.strip_prefix("//?/UNC/") {
        format!("//{v}")
    } else {
        text.strip_prefix("//?/").unwrap_or(&text).to_owned()
    };
    sha(text.to_lowercase().as_bytes())
}
pub fn sha(bytes: &[u8]) -> String {
    hex(&sha_bytes(bytes))
}
pub fn sha_bytes(bytes: &[u8]) -> Vec<u8> {
    Sha256::digest(bytes).to_vec()
}
pub fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
fn unhex(text: &str) -> Result<Vec<u8>> {
    if text.len() != 64
        || !text
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err("Supervisor exact digest encoding".into());
    }
    Ok((0..64)
        .step_by(2)
        .map(|i| u8::from_str_radix(&text[i..i + 2], 16))
        .collect::<std::result::Result<Vec<_>, _>>()?)
}
pub fn root_for(database: &Path, managed: bool) -> Result<PathBuf> {
    Ok(if managed {
        database.canonicalize()?
    } else {
        database.parent().ok_or("database parent")?.canonicalize()?
    })
}
fn validate(r: &OwnerRecord) -> Result<()> {
    codec::validate(r)
}
fn decode(bytes: Vec<u8>, digest: Vec<u8>) -> Result<OwnerRecord> {
    if sha_bytes(&bytes) != digest {
        return Err("Supervisor ownership digest mismatch; no automatic repair".into());
    }
    codec::decode_owner(&bytes)
}
fn reject_legacy(root: &Path) -> Result<()> {
    if root.join(LEGACY_MARKER_NAME).exists() {
        return Err("Legacy supervisor ledger requires a separately reviewed offline migration. Preserve original owners; no automatic migration, clearing or takeover.".into());
    }
    Ok(())
}
impl Owner {
    pub fn claim(root: &Path, host: &Path, package: &Path) -> Result<Self> {
        reject_legacy(root)?;
        let marker = root.join(MARKER_NAME);
        let path = root.join("workbench-supervisor.sqlite");
        let fresh = !marker.exists();
        if !fresh && std::fs::read(&marker)? != MARKER {
            return Err("Supervisor marker changed".into());
        }
        if fresh && path.exists() {
            return Err(
                "Supervisor marker missing from existing ledger; no migration or takeover".into(),
            );
        }
        if !fresh && !path.exists() {
            return Err("Supervisor owner ledger missing; no automatic repair".into());
        }
        if fresh {
            use std::io::Write;
            let mut f = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&marker)?;
            f.write_all(MARKER)?;
            f.sync_all()?;
            std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&path)?
                .sync_all()?;
        }
        let mut db = Connection::open(path)?;
        db.busy_timeout(std::time::Duration::from_millis(100))?;
        if fresh {
            db.execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=FULL; PRAGMA user_version=2; CREATE TABLE owner(singleton INTEGER PRIMARY KEY CHECK(singleton=1),record BLOB NOT NULL CHECK(typeof(record)='blob'),digest BLOB NOT NULL CHECK(typeof(digest)='blob' AND length(digest)=32));")?;
        } else {
            db.execute_batch("PRAGMA synchronous=FULL;")?;
        }
        schema(&db)?;
        let tx = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let profile = unhex(&profile(root))?;
        let old = tx.query_row(
            "SELECT record,digest FROM owner WHERE singleton=1",
            [],
            |r| Ok((r.get::<_, Vec<u8>>(0)?, r.get::<_, Vec<u8>>(1)?)),
        );
        let generation = match old {
            Ok((bytes, digest)) => {
                let r = decode(bytes, digest)?;
                recovery::validate_history_binding(&tx, &r)?;
                if codec::identity(&r)?.profile != profile {
                    return Err("Supervisor library identity changed; no takeover".into());
                }
                if r.phase == OwnerPhase::Recovered as i32 {
                    recovery::validate_recovered(&tx, &r)?;
                } else if r.phase != OwnerPhase::Released as i32 {
                    return Err("Supervisor owner retained. Original resources lack trusted durable recovery proof; no replay or takeover.".into());
                }
                codec::identity(&r)?
                    .generation
                    .checked_add(1)
                    .ok_or("owner generation exhausted")?
            }
            Err(rusqlite::Error::QueryReturnedNoRows) if fresh => 1,
            Err(e) => return Err(e.into()),
        };
        recovery::create_history(&tx)?;
        let mut random = [0; 32];
        getrandom::fill(&mut random)?;
        let record = OwnerRecord {
            model_version: 2,
            identity: Some(Identity {
                profile,
                generation,
                incarnation: sha_bytes(&random),
                host_sha256: sha_bytes(&std::fs::read(host)?),
                package_sha256: sha_bytes(&std::fs::read(package)?),
                supervisor_sha256: sha_bytes(&std::fs::read(std::env::current_exe()?)?),
                supervisor_pid: std::process::id(),
                supervisor_creation_filetime: morrow_native_pipe_win::job::ProcessWatch::open(
                    std::process::id(),
                )?
                .creation_filetime()?,
                child_pid: 0,
                child_creation_filetime: 0,
                recovery_history_head: recovery::head(&tx)?,
            }),
            phase: OwnerPhase::Pending as i32,
            resources: Some(ResourceFacts::default()),
            business_outcome: BusinessOutcome::Unknown as i32,
            protocol_ack: ProtocolAck::NotApplicable as i32,
            reader_joins: None,
            recovery: None,
        };
        let encoded = codec::encode_owner(&record)?;
        tx.execute("INSERT INTO owner VALUES(1,?1,?2) ON CONFLICT(singleton) DO UPDATE SET record=excluded.record,digest=excluded.digest",params![encoded,sha_bytes(&encoded)])?;
        readback(&tx, &encoded)?;
        recovery::validate_history_binding(&tx, &record)?;
        schema(&tx)?;
        tx.commit()?;
        Ok(Self {
            db,
            record,
            encoded,
        })
    }
    pub fn record(&self) -> OwnerRecord {
        self.record.clone()
    }
    pub fn update(&mut self, record: OwnerRecord) -> Result<()> {
        codec::validate_transition(&self.record, &record)?;
        schema(&self.db)?;
        let encoded = codec::encode_owner(&record)?;
        let tx = self
            .db
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let (actual, digest) = read_row(&tx)?;
        if actual != self.encoded
            || digest != sha_bytes(&self.encoded)
            || decode(actual, digest)? != self.record
        {
            return Err("Supervisor exact original owner bytes changed; no release".into());
        }
        recovery::validate_history_binding(&tx, &self.record)?;
        if tx.execute(
            "UPDATE owner SET record=?1,digest=?2 WHERE singleton=1 AND record=?3 AND digest=?4",
            params![
                encoded,
                sha_bytes(&encoded),
                self.encoded,
                sha_bytes(&self.encoded)
            ],
        )? != 1
        {
            return Err("Supervisor owner CAS refused".into());
        }
        readback(&tx, &encoded)?;
        recovery::validate_history_binding(&tx, &record)?;
        schema(&tx)?;
        tx.commit()?;
        self.record = record;
        self.encoded = encoded;
        Ok(())
    }
}
/// Direct host invocations cannot bypass a retained or legacy product owner.
pub fn guard(database: &Path, managed: bool, incarnation: Option<&str>) -> Result<()> {
    let root = root_for(database, managed)?;
    reject_legacy(&root)?;
    let path = root.join("workbench-supervisor.sqlite");
    let marker = root.join(MARKER_NAME);
    if !path.exists() && !marker.exists() {
        return Ok(());
    }
    if !path.exists() || !marker.exists() || std::fs::read(marker)? != MARKER {
        return Err("Supervisor ownership marker or ledger missing; no automatic unlock".into());
    }
    let db = Connection::open_with_flags(path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    schema(&db)?;
    let (bytes, digest) = read_row(&db)?;
    let r = decode(bytes, digest)?;
    recovery::validate_history_binding(&db, &r)?;
    if r.phase == OwnerPhase::Recovered as i32 {
        recovery::validate_recovered(&db, &r)?;
    }
    let id = codec::identity(&r)?;
    if id.profile != unhex(&profile(&root))? {
        return Err("Supervisor library identity mismatch".into());
    }
    if r.phase == OwnerPhase::Released as i32 && incarnation.is_none() {
        return Ok(());
    }
    let token = hex(&id.incarnation);
    if r.phase == OwnerPhase::Active as i32
        && !codec::facts(&r)?.business_gate_revoked
        && !morrow_native_pipe_win::job::RevocationEvent::open(&token)?.is_revoked()?
        && incarnation.is_some_and(|v| v == token)
        && id.child_pid == std::process::id()
        && id.child_creation_filetime
            == morrow_native_pipe_win::job::ProcessWatch::open(std::process::id())?
                .creation_filetime()?
    {
        return Ok(());
    }
    Err("Supervisor owner retained or binding mismatch; no unlock or replay".into())
}

pub struct HostWatch {
    stop: std::sync::Arc<std::sync::atomic::AtomicBool>,
}
impl HostWatch {
    pub fn start(
        root: PathBuf,
        incarnation: String,
        pids: [u32; 2],
        gate: crate::product_gate::ProductGate,
    ) -> Result<Self> {
        let watches = pids
            .into_iter()
            .map(morrow_native_pipe_win::job::ProcessWatch::open)
            .collect::<std::io::Result<Vec<_>>>()?;
        let event = morrow_native_pipe_win::job::RevocationEvent::open(&incarnation)?;
        let expected = unhex(&incarnation)?;
        let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let stopped = stop.clone();
        std::thread::Builder::new()
            .name("workbench-business-fence".into())
            .spawn(move || {
                let inspect = || -> Result<bool> {
                    let db = Connection::open_with_flags(
                        root.join("workbench-supervisor.sqlite"),
                        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
                    )?;
                    db.busy_timeout(std::time::Duration::from_millis(20))?;
                    schema(&db)?;
                    let (bytes, digest) = read_row(&db)?;
                    let r = decode(bytes, digest)?;
                    Ok(codec::identity(&r)?.incarnation != expected
                        || codec::facts(&r)?.business_gate_revoked)
                };
                while !stopped.load(std::sync::atomic::Ordering::Acquire) {
                    if event.is_revoked().unwrap_or(true)
                        || watches.iter().any(|p| p.is_exited().unwrap_or(true))
                        || inspect().unwrap_or(true)
                    {
                        gate.revoke();
                        return;
                    }
                    std::thread::sleep(std::time::Duration::from_millis(5));
                }
            })?;
        Ok(Self { stop })
    }
}
impl Drop for HostWatch {
    fn drop(&mut self) {
        self.stop.store(true, std::sync::atomic::Ordering::Release);
    }
}
pub fn binding(database: &Path, managed: bool, incarnation: &str) -> Result<OwnerRecord> {
    guard(database, managed, Some(incarnation))?;
    let root = root_for(database, managed)?;
    let db = Connection::open_with_flags(
        root.join("workbench-supervisor.sqlite"),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )?;
    let (bytes, digest) = read_row(&db)?;
    let record = decode(bytes, digest)?;
    if codec::identity(&record)?.host_sha256 != sha_bytes(&std::fs::read(std::env::current_exe()?)?)
    {
        return Err("Supervisor host artifact differs from original binding".into());
    }
    Ok(record)
}
#[cfg(test)]
#[path = "supervision_ledger_tests.rs"]
mod ledger_tests;
#[path = "workbench_supervisor.rs"]
mod supervisor_runtime;
pub use supervisor_runtime::run as run_supervisor;
#[doc(hidden)]
pub use supervisor_runtime::run_asuser_parent;
