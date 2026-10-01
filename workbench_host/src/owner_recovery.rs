//! Resource-only recovery. This is never normal shutdown or business completion.
use super::*;
use morrow_native_pipe_win::job::ProcessWatch;
use rusqlite::OpenFlags;

const PROOF: [&str; 7] = ["resource_reclaimed", "gate_closed", "tree_empty", "child_exit", "stdout_eof", "stderr_eof", "control_reaped"];
pub(super) fn complete_abnormal_proof(r: &Value) -> bool {
    r["version"] == 2 && PROOF.iter().all(|k| r[*k] == true)
        && r["business_gate_revoked"] == true && r["normal_shutdown"] == false
        && r["business_outcome"] == "Unknown" && r["protocol_ack"] == "not_applicable"
        && r["child_exit_code"].as_i64().is_some()
}
pub(super) fn create_history(db: &Connection) -> Result<()> {
    db.execute_batch("CREATE TABLE IF NOT EXISTS owner_recovery(sequence INTEGER PRIMARY KEY, original_record TEXT NOT NULL, original_digest TEXT NOT NULL, receipt TEXT NOT NULL, receipt_digest TEXT NOT NULL, previous_head TEXT NOT NULL, head TEXT NOT NULL);")?;
    history_head(db)?;
    Ok(())
}
pub(super) fn head(db: &Connection) -> Result<String> { history_head(db) }
fn history_head(db: &Connection) -> Result<String> {
    let present: i64 = db.query_row("SELECT count(*) FROM sqlite_master WHERE type='table' AND name='owner_recovery'", [], |r| r.get(0))?;
    if present == 0 { return Ok(String::new()); }
    let mut stmt = db.prepare("SELECT sequence,original_record,original_digest,receipt,receipt_digest,previous_head,head FROM owner_recovery ORDER BY sequence")?;
    let mut rows = stmt.query([])?;
    let mut previous = String::new();
    let mut sequence = 0i64;
    while let Some(row) = rows.next()? {
        sequence = sequence.checked_add(1).ok_or("recovery history exhausted")?;
        let actual: i64 = row.get(0)?;
        let original: String = row.get(1)?; let digest: String = row.get(2)?;
        let receipt: String = row.get(3)?; let receipt_digest: String = row.get(4)?;
        let prior: String = row.get(5)?; let head: String = row.get(6)?;
        let r = decode(original.clone(), digest.clone())?;
        let proof: Value = serde_json::from_str(&receipt)?;
        let calculated = sha(json!([actual, original, digest, receipt, receipt_digest, prior]).to_string().as_bytes());
        if actual != sequence || prior != previous || head != calculated
            || r["phase"] != "Closing" || r["recovery_history_head"] != prior || !complete_abnormal_proof(&r)
            || receipt_digest != sha(receipt.as_bytes())
            || proof["version"] != 1 || proof["acknowledged_unknown"] != true
            || proof["original_digest"] != digest || proof["profile"] != r["profile"]
            || proof["generation"] != r["generation"] || proof["incarnation"] != r["incarnation"]
            || proof["manager_sha256"] != r["supervisor_sha256"]
            || !proof["manager_creation_filetime"].as_u64().is_some_and(|v| v>0)
            || !proof["manager_pid"].as_u64().is_some_and(|v| v>0)
            || proof["original_supervisor_ended"] != true {
            return Err("Recovery history integrity or original resource proof is invalid".into());
        }
        previous = head;
    }
    Ok(previous)
}
pub(super) fn validate_history_binding(db: &Connection, r: &Value) -> Result<()> {
    if r["version"] == 2 {
        let present:i64=db.query_row("SELECT count(*) FROM sqlite_master WHERE type='table' AND name='owner_recovery'",[],|row|row.get(0))?;
        if present!=1 {return Err("Version 2 recovery history table is missing; no repair".into());}
    }
    if r["version"] == 2 && r["phase"] != "Recovered" && r["recovery_history_head"] != history_head(db)? {
        return Err("Owner recovery history head is missing or changed".into());
    }
    Ok(())
}
pub(super) fn validate_recovered(db: &Connection, r: &Value) -> Result<()> {
    validate(r)?;
    if r["phase"] != "Recovered" { return Err("Expected resource-only Recovered owner".into()); }
    let current_head = history_head(db)?;
    let (original, digest, receipt_digest, head): (String, String, String, String) = db.query_row(
        "SELECT original_record,original_digest,receipt_digest,head FROM owner_recovery WHERE head=?1",
        [r["recovery"]["history_head"].as_str().ok_or("Missing recovery history binding")?],
        |row| Ok((row.get(0)?,row.get(1)?,row.get(2)?,row.get(3)?)))?;
    let mut expected = decode(original, digest.clone())?;
    expected["phase"] = json!("Recovered");
    expected["recovery"] = json!({"version":1,"original_digest":digest,"receipt_digest":receipt_digest,"history_head":head});
    if *r != expected || current_head != head {
        return Err("Recovered owner differs from the archived original proof".into());
    }
    Ok(())
}
fn open(root: &Path, writable: bool) -> Result<Connection> {
    if std::fs::read(root.join(".morrow-supervision-v1"))? != MARKER {
        return Err("Supervisor marker is missing or changed; preserve evidence".into());
    }
    // No CREATE: preview and repair cannot accidentally initialize a new library.
    let db = Connection::open_with_flags(root.join("workbench-supervisor.sqlite"),
        if writable { OpenFlags::SQLITE_OPEN_READ_WRITE } else { OpenFlags::SQLITE_OPEN_READ_ONLY })?;
    db.busy_timeout(std::time::Duration::from_millis(100))?;
    if writable { db.execute_batch("PRAGMA synchronous=FULL;")?; }
    schema(&db)?;
    Ok(db)
}
fn read(db: &Connection) -> Result<(String, String, Value)> {
    let (text,digest): (String,String) = db.query_row("SELECT record,digest FROM owner WHERE singleton=1", [], |r| Ok((r.get(0)?,r.get(1)?)))?;
    let r = decode(text.clone(), digest.clone())?;
    Ok((text,digest,r))
}
/// This only excludes a still-live writer; it does not supply any resource proof.
fn original_ended(r: &Value) -> Result<bool> {
    let pid = u32::try_from(r["supervisor_pid"].as_u64().ok_or("supervisor PID")?)?;
    match ProcessWatch::open(pid) {
        Ok(p) => Ok(p.creation_filetime()? != r["supervisor_creation_filetime"].as_u64().ok_or("supervisor birth")? || p.is_exited()?),
        // Windows OpenProcess reports ERROR_INVALID_PARAMETER for a PID no longer present.
        // Access denied and every other error remain unconfirmed.
        Err(e) if e.raw_os_error() == Some(87) => Ok(true),
        Err(e) => Err(e.into()),
    }
}
fn reason(r: &Value, root: &Path, host: &Path, package: &Path) -> Result<&'static str> {
    if r["profile"] != profile(root) { return Ok("profile_mismatch"); }
    if r["phase"] == "Released" { return Ok("already_normal_released"); }
    if r["phase"] == "Recovered" {
        if r["host_sha256"] != sha(&std::fs::read(host)?) || r["package_sha256"] != sha(&std::fs::read(package)?)
            || r["supervisor_sha256"] != sha(&std::fs::read(std::env::current_exe()?)?) {
            return Ok("artifact_binding_mismatch");
        }
        return Ok("already_resource_recovered");
    }
    if r["version"] != 2 { return Ok("legacy_identity_unprovable"); }
    if r["phase"] != "Closing" || !complete_abnormal_proof(r) { return Ok("original_resource_proof_missing"); }
    if r["host_sha256"] != sha(&std::fs::read(host)?) || r["package_sha256"] != sha(&std::fs::read(package)?)
        || r["supervisor_sha256"] != sha(&std::fs::read(std::env::current_exe()?)?) { return Ok("artifact_binding_mismatch"); }
    if !original_ended(r)? { return Ok("original_supervisor_still_live"); }
    Ok("eligible_resource_only")
}
fn snapshot(db: &Connection, root: &Path, host: &Path, package: &Path) -> Result<Value> {
    schema(db)?;
    let (text,digest,r) = read(db)?;
    let head = history_head(db)?;
    validate_history_binding(db,&r)?;
    if r["phase"] == "Recovered" { validate_recovered(db,&r)?; }
    let reason = reason(&r,root,host,package)?;
    // The token binds raw SQLite text (not a normalized Value), schema and the history head.
    let token = sha(json!([1, profile(root), text, digest, head, reason]).to_string().as_bytes());
    Ok(json!({"version":1,"profile":profile(root),"preview_token":token,"original_digest":digest,"history_head":head,"eligible":reason=="eligible_resource_only","reason":reason,"resource_recovery_confirmed":reason=="already_resource_recovered","recovery_original_digest":if reason=="already_resource_recovered" {r["recovery"]["original_digest"].clone()} else {Value::Null},"record":r,"business_outcome":"Unknown"}))
}
pub fn preview(root: &Path, host: &Path, package: &Path) -> Result<Value> {
    let root = root.canonicalize()?;
    let mut db = open(&root,false)?;
    let tx = db.transaction()?;
    let result = snapshot(&tx,&root,host,package)?;
    tx.commit()?;
    Ok(result)
}
/// Explicit one-shot management transaction; no guest is started and no operation is replayed.
pub fn recover(root: &Path, host: &Path, package: &Path, expected_token: &str, acknowledge_unknown: bool) -> Result<Value> {
    if !acknowledge_unknown { return Err("Explicit acknowledgment of retained Unknown outcomes is required".into()); }
    let root = root.canonicalize()?;
    let mut db = open(&root,true)?;
    let tx = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let preview = snapshot(&tx,&root,host,package)?;
    if preview["preview_token"] != expected_token || preview["eligible"] != true {
        return Err(format!("Owner recovery refused: {}. Preview again; preserve the original owner and evidence.",preview["reason"]).into());
    }
    let (text,digest,mut r) = read(&tx)?;
    let previous_head = history_head(&tx)?;
    let sequence: i64 = tx.query_row("SELECT count(*)+1 FROM owner_recovery", [], |row| row.get(0))?;
    let receipt = json!({"version":1,"acknowledged_unknown":true,"original_digest":digest,"profile":r["profile"],"generation":r["generation"],"incarnation":r["incarnation"],"manager_pid":std::process::id(),"manager_creation_filetime":ProcessWatch::open(std::process::id())?.creation_filetime()?,"manager_sha256":sha(&std::fs::read(std::env::current_exe()?)?),"original_supervisor_ended":true}).to_string();
    let receipt_digest = sha(receipt.as_bytes());
    let head = sha(json!([sequence,text,digest,receipt,receipt_digest,previous_head]).to_string().as_bytes());
    tx.execute("INSERT INTO owner_recovery VALUES(?1,?2,?3,?4,?5,?6,?7)", params![sequence,text,digest,receipt,receipt_digest,previous_head,head])?;
    r["phase"] = json!("Recovered");
    r["recovery"] = json!({"version":1,"original_digest":digest,"receipt_digest":receipt_digest,"history_head":head});
    validate_recovered(&tx,&r)?;
    let recovered = r.to_string();
    if tx.execute("UPDATE owner SET record=?1,digest=?2 WHERE singleton=1 AND record=?3 AND digest=?4",params![recovered,sha(recovered.as_bytes()),text,digest])? != 1 {
        return Err("Owner recovery CAS refused".into());
    }
    readback(&tx,&recovered)?; schema(&tx)?;
    tx.commit()?;
    // A commit/output failure remains unconfirmed. Callers must preview again; no automatic claim.
    let (_,_,actual) = read(&db)?; validate_recovered(&db,&actual)?;
    if actual != r { return Err("Recovery commit result unconfirmed; preview again".into()); }
    Ok(json!({"version":1,"profile":profile(&root),"request_token":expected_token,"original_digest":digest,"history_head":head,"recovered":true,"business_outcome":"Unknown","normal_shutdown":false,"record":actual}))
}

#[cfg(test)]
mod recovery_tests {
    use super::*;
    fn fixture() -> (tempfile::TempDir, PathBuf, PathBuf, Owner) {
        let dir=tempfile::tempdir().unwrap();let host=dir.path().join("host.bin");let package=dir.path().join("package.bin");
        std::fs::write(&host,b"host").unwrap();std::fs::write(&package,b"package").unwrap();
        let owner=Owner::claim(dir.path(),&host,&package).unwrap();(dir,host,package,owner)
    }
    // Ledger contract tests construct proof flags; actual kernel proof is separately qualified.
    fn closing(owner:&mut Owner, simulate_reused_pid:bool) {
        let mut r=owner.record();r["phase"]=json!("Active");r["child_pid"]=json!(1);r["child_creation_filetime"]=json!(1);owner.update(r).unwrap();
        let mut r=owner.record();r["phase"]=json!("Closing");
        for key in PROOF {r[key]=json!(true);}
        r["business_gate_revoked"]=json!(true);r["normal_shutdown"]=json!(false);r["child_exit_code"]=json!(72);owner.update(r).unwrap();
        if simulate_reused_pid {
            let mut r=owner.record();r["supervisor_creation_filetime"]=json!(1);
            let text=r.to_string();owner.db.execute("UPDATE owner SET record=?1,digest=?2",params![text,sha(text.as_bytes())]).unwrap();
        }
    }
    #[test]
    fn live_original_writer_and_each_missing_proof_refuse_repair() {
        let (dir,host,package,mut owner)=fixture();closing(&mut owner,false);
        let p=preview(dir.path(),&host,&package).unwrap();assert_eq!(p["reason"],"original_supervisor_still_live");
        assert!(recover(dir.path(),&host,&package,p["preview_token"].as_str().unwrap(),true).is_err());
        let full=owner.record();
        for key in PROOF.into_iter().chain(["business_gate_revoked"]) {let mut r=full.clone();r[key]=json!(false);assert!(!complete_abnormal_proof(&r),"{key}");}
        let mut r=full.clone();r["normal_shutdown"]=json!(true);assert!(!complete_abnormal_proof(&r));
        let mut r=full;r["version"]=json!(1);assert!(!complete_abnormal_proof(&r));
    }
    #[test]
    fn explicit_recovery_preserves_unknown_and_only_new_supervisor_may_claim() {
        let (dir,host,package,mut owner)=fixture();closing(&mut owner,true);
        let p=preview(dir.path(),&host,&package).unwrap();assert_eq!(p["eligible"],true);
        let token=p["preview_token"].as_str().unwrap();assert!(recover(dir.path(),&host,&package,token,false).is_err());
        let result=recover(dir.path(),&host,&package,token,true).unwrap();
        assert_eq!(result["record"]["phase"],"Recovered");assert_eq!(result["record"]["normal_shutdown"],false);
        assert_eq!(result["record"]["business_outcome"],"Unknown");assert_eq!(result["record"]["generation"],1);
        assert!(guard(&dir.path().join("library.db"),false,None).is_err());
        assert!(recover(dir.path(),&host,&package,token,true).is_err());
        let mut stale=owner.record();stale["phase"]=json!("Released");stale["normal_shutdown"]=json!(true);stale["child_exit_code"]=json!(0);assert!(owner.update(stale).is_err());
        let next=Owner::claim(dir.path(),&host,&package).unwrap();assert_eq!(next.record()["generation"],2);
        assert_ne!(next.record()["incarnation"],result["record"]["incarnation"]);
        let (archived,digest):(String,String)=next.db.query_row("SELECT original_record,original_digest FROM owner_recovery",[],|r|Ok((r.get(0)?,r.get(1)?))).unwrap();
        assert_eq!(digest,p["original_digest"]);let archived=decode(archived,digest).unwrap();assert_eq!(archived,p["record"]);
    }
    #[test]
    fn repreview_confirms_existing_resource_recovery_but_never_normal_release_or_changed_artifacts() {
        let (dir,host,package,mut owner)=fixture();closing(&mut owner,true);
        let original=preview(dir.path(),&host,&package).unwrap();
        recover(dir.path(),&host,&package,original["preview_token"].as_str().unwrap(),true).unwrap();
        let confirmed=preview(dir.path(),&host,&package).unwrap();
        assert_eq!(confirmed["eligible"],false);assert_eq!(confirmed["resource_recovery_confirmed"],true);
        assert_eq!(confirmed["recovery_original_digest"],original["original_digest"]);
        assert_eq!(confirmed["record"]["normal_shutdown"],false);assert_eq!(confirmed["record"]["business_outcome"],"Unknown");
        std::fs::write(&package,b"changed package").unwrap();
        let changed=preview(dir.path(),&host,&package).unwrap();assert_eq!(changed["reason"],"artifact_binding_mismatch");
        assert_eq!(changed["resource_recovery_confirmed"],false);
        std::fs::write(&package,b"package").unwrap();
        let mut next=Owner::claim(dir.path(),&host,&package).unwrap();closing(&mut next,false);
        let mut normal=next.record();normal["phase"]=json!("Released");normal["normal_shutdown"]=json!(true);normal["child_exit_code"]=json!(0);next.update(normal).unwrap();
        let released=preview(dir.path(),&host,&package).unwrap();assert_eq!(released["reason"],"already_normal_released");
        assert_eq!(released["resource_recovery_confirmed"],false);
    }
    #[test]
    fn new_session_cannot_report_normal_release_after_archive_or_table_loss() {
        for drop_table in [false,true] {
            let (dir,host,package,mut owner)=fixture();closing(&mut owner,true);
            let p=preview(dir.path(),&host,&package).unwrap();recover(dir.path(),&host,&package,p["preview_token"].as_str().unwrap(),true).unwrap();
            let mut next=Owner::claim(dir.path(),&host,&package).unwrap();closing(&mut next,false);
            if drop_table {next.db.execute_batch("DROP TABLE owner_recovery").unwrap();}
            else {next.db.execute("DELETE FROM owner_recovery",[]).unwrap();}
            let mut normal=next.record();normal["phase"]=json!("Released");normal["normal_shutdown"]=json!(true);normal["child_exit_code"]=json!(0);
            assert!(next.update(normal).is_err());assert!(Owner::claim(dir.path(),&host,&package).is_err());
        }
    }
    #[test]
    fn raw_text_changed_after_preview_is_rejected_even_when_json_value_matches() {
        let (dir,host,package,mut owner)=fixture();closing(&mut owner,true);
        let p=preview(dir.path(),&host,&package).unwrap();let text=serde_json::to_string_pretty(&p["record"]).unwrap();
        owner.db.execute("UPDATE owner SET record=?1,digest=?2",params![text,sha(text.as_bytes())]).unwrap();
        assert!(recover(dir.path(),&host,&package,p["preview_token"].as_str().unwrap(),true).is_err());
        assert_eq!(owner.db.query_row("SELECT count(*) FROM owner_recovery",[],|r|r.get::<_,i64>(0)).unwrap(),0);
    }
    #[test]
    fn modified_or_deleted_archive_cannot_unlock_recovered_or_next_generation() {
        for deletion in [false,true] {
            let (dir,host,package,mut owner)=fixture();closing(&mut owner,true);
            let p=preview(dir.path(),&host,&package).unwrap();recover(dir.path(),&host,&package,p["preview_token"].as_str().unwrap(),true).unwrap();
            if deletion {owner.db.execute("DELETE FROM owner_recovery",[]).unwrap();}
            else {owner.db.execute("UPDATE owner_recovery SET original_record='{}'",[]).unwrap();}
            assert!(preview(dir.path(),&host,&package).is_err());assert!(Owner::claim(dir.path(),&host,&package).is_err());
            assert!(guard(&dir.path().join("library.db"),false,None).is_err());
        }
    }
}
