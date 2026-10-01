//! Windows product owner ledger. Missing post-crash proof never clears an owner.
use crate::Result;
use rusqlite::{Connection, TransactionBehavior, params};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
pub struct Owner {
    db: Connection,
    record: Value,
}
#[path = "owner_recovery.rs"]
pub mod recovery;
const MARKER: &[u8] = b"Morrow supervised workbench v1\n";
fn schema(db: &Connection) -> Result<()> {
    let version: i64 = db.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    let triggers: i64 = db.query_row(
        "SELECT count(*) FROM sqlite_master WHERE type='trigger'",
        [],
        |r| r.get(0),
    )?;
    if version != 1 || triggers != 0 {
        return Err("Supervisor owner schema changed; retain original evidence".into());
    }
    Ok(())
}
fn readback(db: &Connection, expected: &str) -> Result<()> {
    let (text, digest) = db.query_row(
        "SELECT record,digest FROM owner WHERE singleton=1",
        [],
        |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)),
    )?;
    if text != expected || digest != sha(expected.as_bytes()) {
        return Err("Supervisor durable owner write verification failed".into());
    }
    decode(text, digest)?;
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
    format!("{:x}", Sha256::digest(bytes))
}
pub fn root_for(database: &Path, managed: bool) -> Result<PathBuf> {
    Ok(if managed {
        database.canonicalize()?
    } else {
        database.parent().ok_or("database parent")?.canonicalize()?
    })
}
fn validate(record: &Value) -> Result<()> {
    if ![json!(1), json!(2)].contains(&record["version"])
        || !record["generation"].as_u64().is_some_and(|v| v > 0)
        || !["Pending", "Active", "Closing", "Released", "Recovered"]
            .contains(&record["phase"].as_str().unwrap_or(""))
    {
        return Err(
            "Supervisor ownership record is invalid; retain this library and its evidence".into(),
        );
    }
    for key in ["profile", "incarnation", "host_sha256", "package_sha256"] {
        if !record[key]
            .as_str()
            .is_some_and(|v| v.len() == 64 && v.bytes().all(|b| b.is_ascii_hexdigit()))
        {
            return Err("Supervisor ownership identity is invalid".into());
        }
    }
    if record["version"] == 2 {
        for key in ["supervisor_sha256"] {
            if !record[key].as_str().is_some_and(|v| v.len()==64 && v.bytes().all(|b| b.is_ascii_hexdigit())) {
                return Err("Supervisor producer identity missing".into());
            }
        }
        if !record["supervisor_creation_filetime"].as_u64().is_some_and(|v| v>0)
            || !record["supervisor_pid"].as_u64().is_some_and(|v| v>0 && v<=u32::MAX as u64)
            || (record["phase"] != "Pending" &&
                (!record["child_creation_filetime"].as_u64().is_some_and(|v| v>0)
                 || !record["child_pid"].as_u64().is_some_and(|v| v>0 && v<=u32::MAX as u64))) {
            return Err("Supervisor process creation binding missing".into());
        }
    }
    if record["phase"] == "Recovered" && !recovery::complete_abnormal_proof(record) {
        return Err("Recovered owner lacks original resource-only proof".into());
    }
    if record["phase"] == "Released"
        && (record["resource_reclaimed"] != true
            || record["gate_closed"] != true
            || record["tree_empty"] != true
            || record["child_exit"] != true
            || record["stdout_eof"] != true
            || record["stderr_eof"] != true
            || record["control_reaped"] != true
            || record["normal_shutdown"] != true
            || record["child_exit_code"] != 0)
    {
        return Err("Supervisor ownership release lacks resource proof".into());
    }
    Ok(())
}
fn decode(text: String, digest: String) -> Result<Value> {
    if sha(text.as_bytes()) != digest {
        return Err("Supervisor ownership digest mismatch; no automatic repair".into());
    }
    let r: Value = serde_json::from_str(&text)?;
    validate(&r)?;
    Ok(r)
}
impl Owner {
    pub fn claim(root: &Path, host: &Path, package: &Path) -> Result<Self> {
        let canonical_root=root.canonicalize()?;
        let root=canonical_root.as_path();
        let path = root.join("workbench-supervisor.sqlite");
        let marker = root.join(".morrow-supervision-v1");
        let fresh = !marker.exists();
        if fresh {
            use std::io::Write;
            let mut file = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&marker)?;
            file.write_all(MARKER)?;
            file.sync_all()?;
        } else if std::fs::read(&marker)? != MARKER {
            return Err("Supervisor ownership marker changed".into());
        }
        if !fresh && !path.exists() {
            return Err(
                "Supervisor owner record is missing; no automatic repair or takeover".into(),
            );
        }
        if fresh && path.exists() {
            return Err("Supervisor ownership marker missing from existing ledger".into());
        }
        if fresh {
            std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&path)?
                .sync_all()?;
        }
        let db = Connection::open(path)?;
        db.busy_timeout(std::time::Duration::from_millis(100))?;
        if fresh {
            db.execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=FULL; PRAGMA user_version=1; CREATE TABLE owner(singleton INTEGER PRIMARY KEY CHECK(singleton=1),record TEXT NOT NULL,digest TEXT NOT NULL);")?;
        } else {
            db.execute_batch("PRAGMA synchronous=FULL;")?;
        }
        schema(&db)?;
        let mut this = Self {
            db,
            record: Value::Null,
        };
        let tx = this
            .db
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let old = tx.query_row(
            "SELECT record,digest FROM owner WHERE singleton=1",
            [],
            |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)),
        );
        let profile = profile(root);
        let generation = match old {
            Ok((r, d)) => {
                let r = decode(r, d)?;
                recovery::validate_history_binding(&tx, &r)?;
                if r["profile"] != profile {
                    return Err("Supervisor library identity changed; no takeover".into());
                }
                if r["phase"] == "Recovered" {
                    recovery::validate_recovered(&tx, &r)?;
                } else if r["phase"] != "Released" {
                    return Err(format!("Supervisor owner retained ({}, generation {}). Previous resources have no trusted durable recovery proof. Keep the original library; automatic replay, database clearing and takeover are refused.",r["phase"],r["generation"]).into());
                }
                r["generation"]
                    .as_u64()
                    .unwrap()
                    .checked_add(1)
                    .ok_or("owner generation exhausted")?
            }
            Err(rusqlite::Error::QueryReturnedNoRows) if fresh => 1,
            Err(e) => return Err(e.into()),
        };
        recovery::create_history(&tx)?;
        let supervisor_creation = morrow_native_pipe_win::job::ProcessWatch::open(std::process::id())?.creation_filetime()?;
        let supervisor_sha = sha(&std::fs::read(std::env::current_exe()?)?);
        let mut random = [0; 32];
        getrandom::fill(&mut random)?;
        let record = json!({"version":2,"supervisor_creation_filetime":supervisor_creation,"supervisor_sha256":supervisor_sha,"child_creation_filetime":0,"recovery_history_head":recovery::head(&tx)?,"profile":profile,"generation":generation,"incarnation":sha(&random),"host_sha256":sha(&std::fs::read(host)?),"package_sha256":sha(&std::fs::read(package)?),"supervisor_pid":std::process::id(),"child_pid":0,"phase":"Pending","resource_reclaimed":false,"gate_closed":false,"tree_empty":false,"child_exit":false,"stdout_eof":false,"stderr_eof":false,"control_reaped":false,"business_outcome":"Unknown","protocol_ack":"not_applicable"});
        let text = record.to_string();
        tx.execute("INSERT INTO owner VALUES(1,?1,?2) ON CONFLICT(singleton) DO UPDATE SET record=excluded.record,digest=excluded.digest",params![text,sha(text.as_bytes())])?;
        readback(&tx, &text)?;
        recovery::validate_history_binding(&tx, &record)?;
        schema(&tx)?;
        tx.commit()?;
        this.record = record;
        Ok(this)
    }
    pub fn record(&self) -> Value {
        self.record.clone()
    }
    pub fn update(&mut self, record: Value) -> Result<()> {
        validate(&record)?;
        schema(&self.db)?;
        let previous_phase = self.record["phase"].as_str().unwrap();
        let next_phase = record["phase"].as_str().unwrap();
        if !matches!(
            (previous_phase, next_phase),
            ("Pending", "Active")
                | ("Active", "Closing")
                | ("Closing", "Closing")
                | ("Closing", "Released")
        ) {
            return Err("Supervisor owner phase transition refused".into());
        }
        if previous_phase != "Pending" && (record["child_pid"] != self.record["child_pid"] || record["child_creation_filetime"] != self.record["child_creation_filetime"]) {
            return Err("Supervisor child identity changed".into());
        }
        for key in [
            "version",
            "profile",
            "generation",
            "incarnation",
            "host_sha256",
            "package_sha256",
            "supervisor_pid",
            "supervisor_creation_filetime",
            "supervisor_sha256",
            "recovery_history_head",
        ] {
            if record[key] != self.record[key] {
                return Err("Supervisor immutable owner binding changed".into());
            }
        }
        let text = record.to_string();
        let previous = self.record.to_string();
        let tx = self
            .db
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let actual = tx.query_row(
            "SELECT record,digest FROM owner WHERE singleton=1",
            [],
            |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)),
        )?;
        if decode(actual.0, actual.1)? != self.record {
            return Err("Supervisor owner changed during session; no release".into());
        }
        recovery::validate_history_binding(&tx, &self.record)?;
        if tx.execute(
            "UPDATE owner SET record=?1,digest=?2 WHERE singleton=1 AND record=?3",
            params![text, sha(text.as_bytes()), previous],
        )? != 1
        {
            return Err("Supervisor owner CAS refused".into());
        }
        readback(&tx, &text)?;
        recovery::validate_history_binding(&tx, &record)?;
        schema(&tx)?;
        tx.commit()?;
        self.record = record;
        Ok(())
    }
}
/// Direct host/maintenance invocations cannot bypass a retained product owner.
pub fn guard(database: &Path, managed: bool, incarnation: Option<&str>) -> Result<()> {
    let root = root_for(database, managed)?;
    let path = root.join("workbench-supervisor.sqlite");
    let marker = root.join(".morrow-supervision-v1");
    if !path.exists() && !marker.exists() {
        return Ok(());
    }
    if !path.exists() || !marker.exists() || std::fs::read(marker)? != MARKER {
        return Err("Supervisor ownership record or marker missing; no automatic unlock".into());
    }
    let db = Connection::open_with_flags(path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    schema(&db)?;
    let (text, digest) = db.query_row(
        "SELECT record,digest FROM owner WHERE singleton=1",
        [],
        |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)),
    )?;
    let r = decode(text, digest)?;
    recovery::validate_history_binding(&db, &r)?;
    if r["phase"] == "Recovered" { recovery::validate_recovered(&db, &r)?; }
    if r["profile"] != profile(&root) {
        return Err("Supervisor library identity mismatch".into());
    }
    if r["phase"] == "Released" && incarnation.is_none() {
        return Ok(());
    }
    if r["phase"] == "Active"
        && r["business_gate_revoked"] != true
        && !morrow_native_pipe_win::job::RevocationEvent::open(
            r["incarnation"].as_str().ok_or("owner incarnation")?,
        )?
        .is_revoked()?
        && incarnation.is_some_and(|v| r["incarnation"] == v)
        && r["child_pid"] == std::process::id()
        && (r["version"] == 1 || r["child_creation_filetime"] == morrow_native_pipe_win::job::ProcessWatch::open(std::process::id())?.creation_filetime()?)
    {
        return Ok(());
    }
    Err("Supervisor owner retained or binding mismatch. Keep the original library; no automatic unlock or replay.".into())
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
                    let (text, digest) = db.query_row(
                        "SELECT record,digest FROM owner WHERE singleton=1",
                        [],
                        |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)),
                    )?;
                    let r = decode(text, digest)?;
                    Ok(r["incarnation"] != incarnation || r["business_gate_revoked"] == true)
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

pub fn binding(database: &Path, managed: bool, incarnation: &str) -> Result<Value> {
    guard(database, managed, Some(incarnation))?;
    let root = root_for(database, managed)?;
    let db = Connection::open_with_flags(
        root.join("workbench-supervisor.sqlite"),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )?;
    let (text, digest) = db.query_row(
        "SELECT record,digest FROM owner WHERE singleton=1",
        [],
        |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)),
    )?;
    let record = decode(text, digest)?;
    if record["host_sha256"] != sha(&std::fs::read(std::env::current_exe()?)?) {
        return Err("Supervisor host artifact differs from owner binding".into());
    }
    Ok(record)
}

#[cfg(test)]
mod ledger_tests {
    use super::*;
    fn fixture() -> (tempfile::TempDir, PathBuf, PathBuf) {
        let dir=tempfile::tempdir().unwrap();
        let host=dir.path().join("host.bin");let package=dir.path().join("package.bin");
        std::fs::write(&host,b"bound host artifact").unwrap();
        std::fs::write(&package,b"bound package artifact").unwrap();
        (dir,host,package)
    }
    fn active(owner:&mut Owner) {
        let mut r=owner.record();r["phase"]=json!("Active");r["child_pid"]=json!(101);r["child_creation_filetime"]=json!(1);owner.update(r).unwrap();
    }
    fn closing(owner:&mut Owner) {
        let mut r=owner.record();r["phase"]=json!("Closing");r["gate_closed"]=json!(true);owner.update(r).unwrap();
    }
    #[test]
    fn concurrent_claims_and_direct_host_cannot_take_active_owner() {
        let (dir,host,package)=fixture();let mut owner=Owner::claim(dir.path(),&host,&package).unwrap();active(&mut owner);
        let before=owner.record();assert!(Owner::claim(dir.path(),&host,&package).is_err());
        assert!(guard(&dir.path().join("library.db"),false,None).is_err());
        readback(&owner.db,&before.to_string()).unwrap();
    }
    #[test]
    fn release_requires_original_complete_normal_proof_then_generation_increases() {
        let (dir,host,package)=fixture();let mut owner=Owner::claim(dir.path(),&host,&package).unwrap();active(&mut owner);closing(&mut owner);
        let mut r=owner.record();r["phase"]=json!("Released");
        assert!(owner.update(r.clone()).is_err());
        for key in ["resource_reclaimed","gate_closed","tree_empty","child_exit","stdout_eof","stderr_eof","control_reaped","normal_shutdown"] {r[key]=json!(true);}
        r["child_exit_code"]=json!(0);
        for key in ["resource_reclaimed","tree_empty","child_exit","stdout_eof","stderr_eof","control_reaped","normal_shutdown"] {
            let mut missing=r.clone();missing[key]=json!(false);assert!(owner.update(missing).is_err(),"{key}");
        }
        let mut abnormal=r.clone();abnormal["child_exit_code"]=json!(2);assert!(owner.update(abnormal).is_err());
        owner.update(r).unwrap();drop(owner);
        let next=Owner::claim(dir.path(),&host,&package).unwrap();assert_eq!(next.record()["generation"],2);
    }
    #[test]
    fn deleted_owner_row_and_missing_ledger_fail_closed() {
        let (dir,host,package)=fixture();let owner=Owner::claim(dir.path(),&host,&package).unwrap();
        owner.db.execute("DELETE FROM owner",[]).unwrap();drop(owner);
        assert!(Owner::claim(dir.path(),&host,&package).is_err());
        std::fs::remove_file(dir.path().join("workbench-supervisor.sqlite")).unwrap();
        assert!(Owner::claim(dir.path(),&host,&package).is_err());
    }
    #[test]
    fn changed_binding_digest_or_schema_cannot_persist_release() {
        for mutation in 0..3 {
            let (dir,host,package)=fixture();let mut owner=Owner::claim(dir.path(),&host,&package).unwrap();active(&mut owner);
            let before=owner.record();let db=Connection::open(dir.path().join("workbench-supervisor.sqlite")).unwrap();
            match mutation {
                0=>{let mut changed=before.clone();changed["generation"]=json!(9);let text=changed.to_string();db.execute("UPDATE owner SET record=?1,digest=?2",params![text,sha(text.as_bytes())]).unwrap();},
                1=>{db.execute("UPDATE owner SET digest='bad'",[]).unwrap();},
                _=>{db.execute_batch("CREATE TRIGGER injected AFTER UPDATE ON owner BEGIN UPDATE owner SET digest='bad'; END;").unwrap();},
            }
            let mut closing=before;closing["phase"]=json!("Closing");assert!(owner.update(closing).is_err());
            assert!(Owner::claim(dir.path(),&host,&package).is_err());
        }
    }
    #[test]
    fn changed_profile_or_marker_cannot_become_fresh_owner() {
        let (dir,host,package)=fixture();let owner=Owner::claim(dir.path(),&host,&package).unwrap();drop(owner);
        std::fs::write(dir.path().join(".morrow-supervision-v1"),b"replacement").unwrap();
        assert!(Owner::claim(dir.path(),&host,&package).is_err());
        std::fs::remove_file(dir.path().join(".morrow-supervision-v1")).unwrap();
        assert!(Owner::claim(dir.path(),&host,&package).is_err());
    }
}
