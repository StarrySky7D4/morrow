//! Abnormal ledger confirmation only; kernel/reader observations are supplied
//! by a future trusted supervisor integration, not created by this module.
//! There is intentionally no runtime caller in this source stage.
use super::{Owner, Result, decode, json, readback, recovery, schema, sha};
use serde_json::Value;

const JOIN_FIELDS: [&str; 3] = [
    "stdout_reader_joined",
    "stderr_reader_joined",
    "controller_reader_joined",
];
const IDENTITY_FIELDS: [&str; 11] = [
    "profile", "generation", "incarnation", "host_sha256",
    "package_sha256", "supervisor_sha256", "supervisor_pid",
    "supervisor_creation_filetime", "child_pid", "child_creation_filetime",
    "recovery_history_head",
];

// This is a confirmation token for one committed abnormal record. It is not
// normal Released, kernel proof, supervisor exit, or authority to claim again.
// No Clone/Debug/Deserialize or public constructor permits reconstruction.
#[allow(dead_code)] // Not wired until actual resource/join producers are added.
pub(crate) struct AbnormalClosurePrepared {
    extension: Value,
}

#[allow(dead_code)]
impl AbnormalClosurePrepared {
    pub(crate) fn into_extension(self) -> Value { self.extension }
}

fn validate_abnormal_record(record: &Value) -> Result<()> {
    super::validate(record)?;
    if record["phase"] != "Closing" || !recovery::complete_abnormal_proof(record) {
        return Err("Abnormal original resource proof is incomplete; retain Unknown".into());
    }
    let joins = record["abnormal_resource_closure_proof"].as_object()
        .ok_or("Original reader join proof is missing; retain Unknown")?;
    if joins.len() != 4 || joins.get("version") != Some(&json!(1))
        || !JOIN_FIELDS.iter().all(|key| joins.get(*key) == Some(&json!(true)))
    {
        return Err("Original reader joins are unconfirmed; retain Unknown".into());
    }
    for key in ["profile", "incarnation", "host_sha256", "package_sha256", "supervisor_sha256"] {
        if !record[key].as_str().is_some_and(|v| v.len() == 64
            && v.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)))
        {
            return Err("Abnormal original identity encoding is invalid".into());
        }
    }
    let head = record["recovery_history_head"].as_str()
        .ok_or("Original history binding is missing")?;
    if !(head.is_empty() || (head.len() == 64
        && head.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))))
    {
        return Err("Original history binding is invalid".into());
    }
    Ok(())
}

// A new read transaction after the write transaction has committed. Neither
// in-memory Owner.record nor an earlier transaction readback substitutes for it.
fn confirm_committed(owner: &Owner, expected: &Value) -> Result<AbnormalClosurePrepared> {
    validate_abnormal_record(expected)?;
    let text = expected.to_string();
    let tx = owner.db.unchecked_transaction()?;
    schema(&tx)?;
    readback(&tx, &text)?;
    recovery::validate_history_binding(&tx, expected)?;
    let (actual_text, digest): (String, String) = tx.query_row(
        "SELECT record,digest FROM owner WHERE singleton=1", [],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;
    let actual = decode(actual_text.clone(), digest.clone())?;
    if actual != *expected || actual_text != text || digest != sha(text.as_bytes()) {
        return Err("Abnormal commit readback is unconfirmed; retain Unknown".into());
    }
    validate_abnormal_record(&actual)?;
    // A failure closing this read transaction also returns no prepared token.
    tx.commit()?;
    let mut identity = serde_json::Map::new();
    for key in IDENTITY_FIELDS { identity.insert(key.into(), actual[key].clone()); }
    Ok(AbnormalClosurePrepared { extension: json!({
        "version": 1,
        "kind": "AbnormalResourceClosurePreparedV1",
        "identity": identity,
        "committed_record_sha256": digest,
        "child_exit_code": actual["child_exit_code"],
        "resource_reclaimed": true,
        "gate_closed": true,
        "business_gate_revoked": true,
        "tree_empty": true,
        "child_exit": true,
        "stdout_eof": true,
        "stderr_eof": true,
        "control_reaped": true,
        "reader_joins": actual["abnormal_resource_closure_proof"],
        "durable_abnormal_record_confirmed": true,
        "normal_shutdown": false,
        "owner_phase": "Retained",
        "business_outcome": "Unknown",
        "protocol_ack": "not_applicable",
        "accepted": false,
        "supervisor_exit_confirmed": false
    }) })
}

impl Owner {
    /// Future trusted producer only. These input fields must come from actual
    /// original child/Job/EOF/reap/join observations; this validator cannot
    /// turn a caller-supplied boolean into kernel evidence. No caller is wired.
    #[allow(dead_code)]
    pub(crate) fn prepare_abnormal_resource_closure(
        &mut self, record: Value,
    ) -> Result<AbnormalClosurePrepared> {
        validate_abnormal_record(&record)?;
        // Original immutable identity, exact-row CAS, recovery history,
        // transactional readback and commit are all checked by unchanged update.
        self.update(record.clone())?;
        // Any error after commit remains unconfirmed; do not retry or claim a
        // replacement owner. The durable Closing row remains retained.
        confirm_committed(self, &record)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::{Connection, params};

    fn fixture() -> Owner {
        let db = Connection::open_in_memory().unwrap();
        db.execute_batch("PRAGMA user_version=1; PRAGMA synchronous=FULL; CREATE TABLE owner(singleton INTEGER PRIMARY KEY CHECK(singleton=1),record TEXT NOT NULL,digest TEXT NOT NULL);").unwrap();
        recovery::create_history(&db).unwrap();
        let record = json!({"version":2,"profile":"11".repeat(32),"generation":1,
            "incarnation":"22".repeat(32),"host_sha256":"33".repeat(32),
            "package_sha256":"44".repeat(32),"supervisor_sha256":"55".repeat(32),
            "supervisor_pid":41,"supervisor_creation_filetime":100,
            "child_pid":42,"child_creation_filetime":101,"recovery_history_head":"",
            "phase":"Active","business_outcome":"Unknown","protocol_ack":"not_applicable"});
        let text = record.to_string();
        db.execute("INSERT INTO owner VALUES(1,?1,?2)",params![text,sha(text.as_bytes())]).unwrap();
        Owner { db, record }
    }
    // These are synthetic ledger facts, deliberately not Windows resource proof.
    fn closing(owner: &Owner) -> Value {
        let mut r = owner.record(); r["phase"] = json!("Closing");
        for key in ["resource_reclaimed","gate_closed","tree_empty","child_exit",
            "stdout_eof","stderr_eof","control_reaped","business_gate_revoked"] {
            r[key] = json!(true);
        }
        r["normal_shutdown"] = json!(false); r["child_exit_code"] = json!(2);
        r["abnormal_resource_closure_proof"] = json!({"version":1,
            "stdout_reader_joined":true,"stderr_reader_joined":true,
            "controller_reader_joined":true});
        r
    }
    fn persisted(owner: &Owner) -> (String, String) {
        owner.db.query_row("SELECT record,digest FROM owner WHERE singleton=1",[],
            |row| Ok((row.get(0)?,row.get(1)?))).unwrap()
    }
    #[test]
    fn committed_abnormal_receipt_retains_unknown_owner_and_never_claims_exit() {
        let mut owner = fixture(); let r = closing(&owner);
        let prepared = owner.prepare_abnormal_resource_closure(r.clone()).unwrap().into_extension();
        assert_eq!(owner.record(), r);
        assert_eq!(prepared["committed_record_sha256"], sha(r.to_string().as_bytes()));
        assert_eq!(prepared["identity"]["child_creation_filetime"], r["child_creation_filetime"]);
        assert_eq!(prepared["owner_phase"], "Retained");
        assert_eq!(prepared["business_outcome"], "Unknown");
        assert_eq!(prepared["normal_shutdown"], false);
        assert_eq!(prepared["accepted"], false);
        assert_eq!(prepared["supervisor_exit_confirmed"], false);
        assert_eq!(owner.record()["phase"], "Closing");
    }
    #[test]
    fn each_missing_resource_or_join_fact_refuses_before_ledger_effects() {
        let mut owner = fixture(); let full = closing(&owner); let before = persisted(&owner);
        for key in ["resource_reclaimed","gate_closed","tree_empty","child_exit",
            "stdout_eof","stderr_eof","control_reaped","business_gate_revoked"] {
            let mut r=full.clone(); r[key]=json!(false);
            assert!(owner.prepare_abnormal_resource_closure(r).is_err(), "{key}");
            assert_eq!(persisted(&owner), before);
        }
        for key in JOIN_FIELDS {
            for bad in [Value::Null,json!(false),json!("true"),json!(1)] {
                let mut r=full.clone(); r["abnormal_resource_closure_proof"][key]=bad;
                assert!(owner.prepare_abnormal_resource_closure(r).is_err(), "{key}");
                assert_eq!(persisted(&owner), before);
            }
        }
    }
    #[test]
    fn legacy_normal_and_changed_identity_cannot_prepare_quarantine() {
        let mut owner=fixture(); let full=closing(&owner); let before=persisted(&owner);
        let mut legacy=full.clone(); legacy["version"]=json!(1);
        assert!(owner.prepare_abnormal_resource_closure(legacy).is_err());
        let mut normal=full.clone(); normal["normal_shutdown"]=json!(true);
        assert!(owner.prepare_abnormal_resource_closure(normal).is_err());
        for key in IDENTITY_FIELDS {
            let mut changed=full.clone();
            changed[key]=match key {
                "generation"|"supervisor_pid"|"child_pid" => json!(99),
                "supervisor_creation_filetime"|"child_creation_filetime" => json!(999),
                _ => json!("aa".repeat(32)),
            };
            assert!(owner.prepare_abnormal_resource_closure(changed).is_err(), "{key}");
            assert_eq!(persisted(&owner), before);
        }
    }
    #[test]
    fn stale_original_row_and_postcommit_tamper_are_unconfirmed() {
        let mut owner=fixture(); let r=closing(&owner);
        let mut other=owner.record(); other["incarnation"]=json!("aa".repeat(32));
        let text=other.to_string();
        owner.db.execute("UPDATE owner SET record=?1,digest=?2",params![text,sha(text.as_bytes())]).unwrap();
        assert!(owner.prepare_abnormal_resource_closure(r).is_err());
        assert_eq!(persisted(&owner).0, other.to_string());
        let mut owner=fixture(); let r=closing(&owner);
        owner.prepare_abnormal_resource_closure(r.clone()).unwrap();
        owner.db.execute("UPDATE owner SET digest='bad'",[]).unwrap();
        assert!(confirm_committed(&owner,&r).is_err());
        // Reconfirmation failure never repairs or rewrites the durable row.
        assert_eq!(persisted(&owner).1,"bad");
    }
    #[test]
    fn normal_v1_v2_owner_update_paths_remain_separate() {
        for version in [1, 2] {
            let mut owner=fixture();
            // Synthetic legacy/current starting rows, not a production upgrade.
            owner.record["version"]=json!(version);
            let text=owner.record.to_string();
            owner.db.execute("UPDATE owner SET record=?1,digest=?2",
                params![text,sha(text.as_bytes())]).unwrap();
            let mut r=closing(&owner);
            r.as_object_mut().unwrap().remove("abnormal_resource_closure_proof");
            r["normal_shutdown"]=json!(true); r["child_exit_code"]=json!(0);
            owner.update(r.clone()).unwrap();
            r["phase"]=json!("Released"); owner.update(r.clone()).unwrap();
            assert_eq!(owner.record(),r);
            let before=persisted(&owner);
            assert!(owner.prepare_abnormal_resource_closure(r).is_err());
            assert_eq!(persisted(&owner),before);
        }
    }
}
