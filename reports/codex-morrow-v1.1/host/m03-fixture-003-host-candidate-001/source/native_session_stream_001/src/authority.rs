//! Trusted native-domain extension. Persisted records never reconstruct live approval.
use crate::{Admission, LaunchSpec, NativeHost, Result, Session, Snapshot, wire};
use morrow_core::store::{EventBudget, Store};
use prost::Message;
use rusqlite::{Connection, OpenFlags, OptionalExtension, TransactionBehavior, params};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
pub mod proto {
    include!(concat!(env!("OUT_DIR"), "/morrow.native_admission.v1.rs"));
}
const MAX: usize = 8192;
const MAGIC: &[u8; 8] = b"MRNADM04";
fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}
fn random() -> Result<String> {
    let mut b = [0; 32];
    getrandom::fill(&mut b).map_err(err)?;
    Ok(wire::hex(&b))
}
fn valid(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 128
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b))
}
// Same bounded Protobuf/LZ4 envelope layout as core, with a distinct domain magic.
pub(crate) fn pack<T: Message>(v: &T) -> Result<Vec<u8>> {
    let raw = v.encode_to_vec();
    if raw.len() > MAX {
        return Err("record limit".into());
    }
    let compressed = lz4_flex::block::compress(&raw);
    let mut out = MAGIC.to_vec();
    out.extend(1u16.to_le_bytes());
    out.extend((raw.len() as u32).to_le_bytes());
    out.extend((compressed.len() as u32).to_le_bytes());
    out.extend(wire::digest(&raw));
    out.extend(compressed);
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn setup() -> (HostAuthority, LaunchSpec) {
        let root =
            PathBuf::from(std::env::var("MORROW_OWNER_TEST_ROOT").unwrap()).join(random().unwrap());
        fs::create_dir(&root).unwrap();
        let profile = root.join("profile");
        fs::create_dir(&profile).unwrap();
        let cwd = root.join("cwd");
        fs::create_dir(&cwd).unwrap();
        let exe = PathBuf::from(std::env::var("MORROW_OWNER_TEST_CLIENT").unwrap());
        HostAuthority::initialize(&profile, "slot").unwrap();
        (
            HostAuthority::open(&profile).unwrap(),
            LaunchSpec {
                slot: "slot".into(),
                artifact_sha256: wire::digest(&fs::read(&exe).unwrap()),
                executable: exe,
                cwd,
                args: vec![],
                ttl_ms: 10000,
                handshake_ms: 2500,
                frame_ms: 500,
                close_ms: 100,
                request_budget: 32,
                http_origin: "http://127.0.0.1:1".into(),
            },
        )
    }
    #[test]
    fn every_fixed_approval_field_is_compared_to_original_host_record() {
        let (mut a, spec) = setup();
        let id = a
            .approve(spec, "plugin-a", "reader", "operation-a")
            .unwrap();
        let old = load_grant(&a.db, &id).unwrap();
        for field in 0..11 {
            let mut changed = old.clone();
            match field {
                0 => changed.plugin_id = "plugin-b".into(),
                1 => changed.role = "writer".into(),
                2 => changed.operation = "operation-b".into(),
                3 => changed.artifact_sha256[0] ^= 1,
                4 => changed.approved_unix_ms += 1,
                5 => changed.lifetime_ms += 1,
                6 => changed.issuer = random().unwrap(),
                7 => changed.profile = random().unwrap(),
                8 => changed.slot = "different".into(),
                9 => changed.config_sha256[0] ^= 1,
                _ => changed.capabilities = 2,
            };
            save_grant(&a.db, &changed).unwrap();
            let failure = a.claim(&id).err().expect("changed record must reject");
            assert!(
                failure.contains("binding") || failure.contains("integrity"),
                "{field}: {failure}"
            );
            assert!(a.snapshot().is_none());
            assert!(load_owner(&a.db).unwrap().is_none());
            save_grant(&a.db, &old).unwrap();
        }
    }
    #[test]
    fn approved_data_cannot_be_claimed_by_new_issuer() {
        let (mut a, spec) = setup();
        let id = a.approve(spec, "plugin", "reader", "op").unwrap();
        let mut other = HostAuthority::open(&a.root).unwrap();
        assert!(other.claim(&id).unwrap_err().contains("no live approval"));
    }
    #[tokio::test]
    async fn revoke_committed_before_claim_prevents_spawn() {
        let (mut a, spec) = setup();
        let id = a.approve(spec, "plugin", "reader", "op").unwrap();
        let mut other = HostAuthority::open(&a.root).unwrap();
        other.revoke(&id).await.unwrap();
        assert!(a.claim(&id).is_err());
        assert!(a.snapshot().is_none());
        assert!(load_owner(&a.db).unwrap().is_none());
    }
}
/// API-test fixture only: records a simulated owner, never spawns a child.
#[cfg(test)]
pub(crate) fn simulated_http_parent() -> crate::http_authority::Parent {
    use std::sync::{Arc, Mutex};
    let root = PathBuf::from(std::env::var("MORROW_HTTP_TEST_ROOT").unwrap())
        .join(&random().unwrap()[..16]);
    fs::create_dir(&root).unwrap();
    let profile = root.join("profile");
    fs::create_dir(&profile).unwrap();
    let cwd = root.join("cwd");
    fs::create_dir(&cwd).unwrap();
    HostAuthority::initialize(&profile, "test-slot").unwrap();
    let mut a = HostAuthority::open(&profile).unwrap();
    let executable = std::env::current_exe().unwrap();
    let id = a
        .approve(
            LaunchSpec {
                slot: "test-slot".into(),
                artifact_sha256: wire::digest(&fs::read(&executable).unwrap()),
                executable,
                cwd,
                args: vec![],
                ttl_ms: 10000,
                handshake_ms: 2500,
                frame_ms: 500,
                close_ms: 100,
                request_budget: 128,
                http_origin: "http://127.0.0.1:1".into(),
            },
            "api-test",
            "http",
            "operation-test",
        )
        .unwrap();
    let live = a.live.remove(&id).unwrap();
    let deadline = live.admission.created + Duration::from_millis(live.admission.spec.ttl_ms);
    let mut approval = live.record;
    approval.state = 2;
    save_grant(&a.db, &approval).unwrap();
    save_owner(
        &a.db,
        &proto::Owner {
            version: 1,
            grant_id: id,
            issuer: a.issuer.clone(),
            generation: approval.generation,
            phase: "Active".into(),
            child_pid: 123,
            session: 456,
            epoch: 1,
            ..Default::default()
        },
    )
    .unwrap();
    let mut store =
        Store::open_existing(&profile.join("coordination.sqlite"), EventBudget::default()).unwrap();
    store.pin_service_authority().unwrap();
    crate::http_authority::Parent {
        pid: 123,
        session: 456,
        epoch: 1,
        root: profile,
        approval,
        store: Arc::new(Mutex::new(store)),
        gate: Arc::new(Mutex::new(crate::pipe_driver::EffectGate {
            revoked: false,
            deadline,
            ordinal: 0,
            last_write: 0,
            issued_body_end: 0,
            network_pending: false,
        })),
        origin: "http://127.0.0.1:1".into(),
        deadline,
    }
}
pub(crate) fn unpack<T: Message + Default>(b: &[u8]) -> Result<T> {
    if b.len() < 50
        || b.len() > MAX + MAX / 255 + 128
        || &b[..8] != MAGIC
        || b[8..10] != 1u16.to_le_bytes()
    {
        return Err("record envelope".into());
    }
    let n = u32::from_le_bytes(b[10..14].try_into().unwrap()) as usize;
    let p = u32::from_le_bytes(b[14..18].try_into().unwrap()) as usize;
    if n > MAX || p != b.len() - 50 {
        return Err("record length".into());
    }
    let raw = lz4_flex::block::decompress(&b[50..], n).map_err(err)?;
    if raw.len() != n || wire::digest(&raw) != b[18..50] {
        return Err("record integrity".into());
    }
    let value = T::decode(raw.as_slice()).map_err(err)?;
    if value.encode_to_vec() != raw {
        return Err("noncanonical record".into());
    }
    Ok(value)
}
fn checked_path(path: &Path) -> Result<PathBuf> {
    if !path.is_absolute() {
        return Err("absolute profile required".into());
    }
    for p in path.ancestors() {
        if p.as_os_str().is_empty() {
            continue;
        }
        let m = fs::symlink_metadata(p).map_err(err)?;
        if m.file_type().is_symlink() {
            return Err("symlink profile rejected".into());
        }
        #[cfg(windows)]
        {
            use std::os::windows::fs::MetadataExt;
            if m.file_attributes() & 0x400 != 0 {
                return Err("reparse profile rejected".into());
            }
        }
    }
    let p = fs::canonicalize(path).map_err(err)?;
    #[cfg(windows)]
    if p.to_string_lossy().starts_with("\\\\?\\UNC\\") {
        return Err("local profile required".into());
    }
    Ok(p)
}
fn namespace() -> Result<String> {
    let p = std::env::var_os("LOCALAPPDATA").ok_or("LOCALAPPDATA required")?;
    Ok(checked_path(Path::new(&p))?.to_string_lossy().into_owned())
}
fn core_identity(root: &Path) -> Result<Vec<u8>> {
    checked_path(&root.join("coordination.sqlite"))?;
    let c = Connection::open_with_flags(
        root.join("coordination.sqlite"),
        OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .map_err(err)?;
    let b:Vec<u8>=c.query_row("SELECT CASE WHEN length(payload)<=192 THEN payload ELSE NULL END FROM service_authority_identity WHERE singleton=1",[],|r|r.get(0)).map_err(err)?;
    Ok(wire::digest(&b).to_vec())
}
pub(crate) fn connect(root: &Path) -> Result<Connection> {
    checked_path(&root.join("native-admissions.sqlite"))?;
    let c = Connection::open_with_flags(
        root.join("native-admissions.sqlite"),
        OpenFlags::SQLITE_OPEN_READ_WRITE,
    )
    .map_err(err)?;
    c.busy_timeout(Duration::from_millis(100)).map_err(err)?;
    c.execute_batch("PRAGMA synchronous=FULL; PRAGMA foreign_keys=ON;")
        .map_err(err)?;
    let version: i64 = c
        .query_row("PRAGMA user_version", [], |r| r.get(0))
        .map_err(err)?;
    if version != 4 {
        return Err("native ledger version".into());
    }
    Ok(c)
}
pub(crate) fn load_owner(c: &Connection) -> Result<Option<proto::Owner>> {
    let b:Option<Vec<u8>>=c.query_row("SELECT CASE WHEN length(payload)<=8352 THEN payload ELSE NULL END FROM owner WHERE singleton=1",[],|r|r.get(0)).optional().map_err(err)?;
    b.map(|b| {
        let o: proto::Owner = unpack(&b)?;
        if o.version != 1
            || o.grant_id.len() != 64
            || o.generation == 0
            || !matches!(
                o.phase.as_str(),
                "LaunchPending"
                    | "Preparing"
                    | "Active"
                    | "Revoked"
                    | "Closing"
                    | "ClosingUnconfirmed"
                    | "Released"
                    | "NotStarted"
                    | "Unknown"
            )
        {
            return Err("owner integrity".into());
        }
        if o.phase == "Released" && !(o.exit_observed && o.stdout_eof && o.stderr_eof) {
            return Err("unproved release".into());
        }
        Ok(o)
    })
    .transpose()
}
pub(crate) fn save_owner(c: &Connection, o: &proto::Owner) -> Result<()> {
    c.execute("INSERT INTO owner VALUES(1,?1) ON CONFLICT(singleton) DO UPDATE SET payload=excluded.payload",[pack(o)?]).map_err(err)?;
    Ok(())
}
pub(crate) fn load_grant(c: &Connection, id: &str) -> Result<proto::Approval> {
    if id.len() != 64 || !id.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err("invalid grant reference".into());
    }
    let (seq,b):(i64,Vec<u8>)=c.query_row("SELECT seq,CASE WHEN length(payload)<=8352 THEN payload ELSE NULL END FROM approvals WHERE id=?1",[id],|r|Ok((r.get(0)?,r.get(1)?))).map_err(err)?;
    let g: proto::Approval = unpack(&b)?;
    if g.version != 1
        || g.id != id
        || g.generation != seq as u64
        || g.issuer.len() != 64
        || g.profile.len() != 64
        || !valid(&g.slot)
        || !valid(&g.plugin_id)
        || !valid(&g.role)
        || !valid(&g.operation)
        || g.artifact_sha256.len() != 32
        || g.config_sha256.len() != 32
        || g.schema_sha256 != wire::schema_digest()
        || g.capabilities != 3
        || !(1..=4).contains(&g.state)
        || !(100..=60000).contains(&g.lifetime_ms)
        || !valid_revocation(&g)
    {
        return Err("approval integrity".into());
    }
    Ok(g)
}
pub(crate) fn valid_revocation(g: &proto::Approval) -> bool {
    if g.state == 3 {
        (g.revocation_source == 1 && g.revocation_reason == 19)
            || (g.revocation_source == 2 && (16..=30).contains(&g.revocation_reason))
    } else {
        g.revocation_source == 0 && g.revocation_reason == 0
    }
}
pub(crate) fn record_revocation(
    db: &Connection,
    g: &mut proto::Approval,
    source: u32,
    reason: u32,
) -> Result<()> {
    if g.state != 3 {
        g.state = 3;
        g.revocation_source = source;
        g.revocation_reason = reason;
        if !valid_revocation(g) {
            return Err("invalid revocation provenance".into());
        }
        save_grant(db, g)?;
    }
    Ok(())
}
pub(crate) fn save_grant(c: &Connection, g: &proto::Approval) -> Result<()> {
    if c.execute(
        "UPDATE approvals SET payload=?1 WHERE id=?2 AND seq=?3",
        params![pack(g)?, g.id, g.generation as i64],
    )
    .map_err(err)?
        != 1
    {
        return Err("grant compare failed".into());
    }
    Ok(())
}
fn terminal(o: &proto::Owner) -> bool {
    matches!(o.phase.as_str(), "Released" | "NotStarted")
}
#[cfg(test)]
pub(crate) fn authority_with_simulated_session(
    parent: &crate::http_authority::Parent,
    shared: std::sync::Arc<crate::Shared>,
) -> (HostAuthority, tokio::sync::mpsc::Receiver<crate::Control>) {
    let mut authority = HostAuthority::open(&parent.root).unwrap();
    let (tx, rx) = tokio::sync::mpsc::channel(8);
    authority.issuer = parent.approval.issuer.clone();
    authority.active = Some(parent.approval.id.clone());
    authority.owner_phase = "Active".into();
    authority.pin = Some(parent.store.clone());
    authority.session = Some(crate::Session {
        shared,
        control: tx,
    });
    (authority, rx)
}

/// One explicit local profile/slot. This is NOT a network authority or UI approval provider.
struct LiveApproval {
    admission: Admission,
    record: proto::Approval,
}
pub struct HostAuthority {
    #[cfg(feature = "qualification-pipe-fault")]
    qualification_plan_issued: bool,
    root: PathBuf,
    profile: proto::Profile,
    db: Connection,
    issuer: String,
    live: BTreeMap<String, LiveApproval>,
    host: NativeHost,
    pin: Option<std::sync::Arc<std::sync::Mutex<Store>>>,
    session: Option<Session>,
    active: Option<String>,
    owner_phase: String,
    failed: bool,
    started: Instant,
    pub events: Vec<Value>,
}
impl HostAuthority {
    pub fn initialize(path: &Path, slot: &str) -> Result<()> {
        if !valid(slot) || slot.len() > 64 {
            return Err("invalid slot".into());
        }
        let root = checked_path(path)?;
        if !root.is_dir() || fs::read_dir(&root).map_err(err)?.next().is_some() {
            return Err("init requires empty profile".into());
        }
        // Only one initializer wins. Never deleted: interrupted initialization is not recreated.
        fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(root.join("init.guard"))
            .map_err(err)?
            .sync_all()
            .map_err(err)?;
        let lock_namespace = namespace()?;
        let _core =
            Store::open(&root.join("coordination.sqlite"), EventBudget::default()).map_err(err)?;
        let p = proto::Profile {
            version: 1,
            identity: random()?,
            slot: slot.into(),
            core_identity_sha256: core_identity(&root)?,
            lock_namespace,
            canonical_root: root.to_string_lossy().into_owned(),
        };
        let mut c = Connection::open(root.join("native-admissions.sqlite")).map_err(err)?;
        c.execute_batch("PRAGMA journal_mode=DELETE; PRAGMA synchronous=FULL;")
            .map_err(err)?;
        let tx = c
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(err)?;
        tx.execute_batch("CREATE TABLE profile(singleton INTEGER PRIMARY KEY CHECK(singleton=1),payload BLOB NOT NULL) STRICT; CREATE TABLE approvals(seq INTEGER PRIMARY KEY AUTOINCREMENT,id TEXT NOT NULL UNIQUE,payload BLOB NOT NULL) STRICT; CREATE TABLE owner(singleton INTEGER PRIMARY KEY CHECK(singleton=1),payload BLOB NOT NULL) STRICT; CREATE TABLE http_approvals(id TEXT PRIMARY KEY,payload BLOB NOT NULL) STRICT; PRAGMA user_version=4;").map_err(err)?;
        tx.execute("INSERT INTO profile VALUES(1,?1)", [pack(&p)?])
            .map_err(err)?;
        tx.commit().map_err(err)?;
        Ok(())
    }
    pub fn open(path: &Path) -> Result<Self> {
        let root = checked_path(path)?;
        let db = connect(&root)?;
        let b:Vec<u8>=db.query_row("SELECT CASE WHEN length(payload)<=8352 THEN payload ELSE NULL END FROM profile WHERE singleton=1",[],|r|r.get(0)).map_err(err)?;
        let profile: proto::Profile = unpack(&b)?;
        if profile.version != 1
            || profile.identity.len() != 64
            || !valid(&profile.slot)
            || profile.core_identity_sha256 != core_identity(&root)?
            || profile.lock_namespace != namespace()?
            || profile.canonical_root != root.to_string_lossy()
        {
            return Err("profile identity, location or lock namespace mismatch".into());
        }
        load_owner(&db)?;
        Ok(Self {
            #[cfg(feature = "qualification-pipe-fault")]
            qualification_plan_issued: false,
            root,
            profile,
            db,
            issuer: random()?,
            live: BTreeMap::new(),
            host: NativeHost::new()?,
            pin: None,
            session: None,
            active: None,
            owner_phase: String::new(),
            failed: false,
            started: Instant::now(),
            events: vec![],
        })
    }
    fn event(&mut self, name: &str, detail: Value) {
        if self.events.len() < 1024 {
            self.events.push(
                json!({"event":name,"at_us":self.started.elapsed().as_micros(),"detail":detail}),
            );
        } else {
            self.failed = true;
        }
    }
    pub fn slot(&self) -> &str {
        &self.profile.slot
    }
    pub fn approve(
        &mut self,
        spec: LaunchSpec,
        plugin: &str,
        role: &str,
        operation: &str,
    ) -> Result<String> {
        self.approve_bound(spec, plugin, role, operation, |_| Ok(()))
    }
    #[cfg(feature = "qualification-pipe-fault")]
    pub fn approve_with_pipe_fault(
        &mut self,
        spec: LaunchSpec,
        plugin: &str,
        role: &str,
        operation: &str,
        plan: crate::PipeFaultPlan,
    ) -> Result<String> {
        if self.qualification_plan_issued {
            return Err("qualification plan already issued; no rearm".into());
        }
        let result = self.approve_bound(spec, plugin, role, operation, move |admission| {
            admission.bind_pipe_fault(plan)
        });
        if result.is_ok() {
            self.qualification_plan_issued = true;
        }
        result
    }
    fn approve_bound(
        &mut self,
        spec: LaunchSpec,
        plugin: &str,
        role: &str,
        operation: &str,
        bind: impl FnOnce(&mut Admission) -> Result<()>,
    ) -> Result<String> {
        if self.failed
            || self.live.len() >= 16
            || spec.slot != self.profile.slot
            || ![plugin, role, operation].into_iter().all(valid)
        {
            return Err("approval boundary".into());
        }
        // Opaque admission holds the first Instant, fixed image/config and private channel nonce.
        let mut admission = Admission::authorize(spec)?;
        bind(&mut admission)?;
        let mut g = proto::Approval {
            version: 1,
            id: random()?,
            issuer: self.issuer.clone(),
            profile: self.profile.identity.clone(),
            slot: self.profile.slot.clone(),
            plugin_id: plugin.into(),
            role: role.into(),
            operation: operation.into(),
            artifact_sha256: admission.spec.artifact_sha256.to_vec(),
            config_sha256: vec![],
            schema_sha256: wire::schema_digest().to_vec(),
            generation: 0,
            capabilities: 3,
            lifetime_ms: admission.spec.ttl_ms,
            approved_unix_ms: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_err(err)?
                .as_millis() as u64,
            state: 1,
            revocation_source: 0,
            revocation_reason: 0,
        };
        let tx = self
            .db
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(err)?;
        let count: i64 = tx
            .query_row("SELECT count(*) FROM approvals", [], |r| r.get(0))
            .map_err(err)?;
        if count >= 512 {
            return Err("approval quota".into());
        }
        tx.execute("INSERT INTO approvals(id,payload) VALUES(?1,X'')", [&g.id])
            .map_err(err)?;
        g.generation = tx.last_insert_rowid() as u64;
        admission.bind_authority(&g.encode_to_vec());
        g.config_sha256 = admission.config.to_vec();
        save_grant(&tx, &g)?;
        tx.commit()
            .map_err(|e| format!("approval commit unknown: {e}"))?;
        self.live.insert(
            g.id.clone(),
            LiveApproval {
                admission,
                record: g.clone(),
            },
        );
        self.event("authorize_created",json!({"grant_id":g.id,"generation":g.generation,"config_sha256":wire::hex(&g.config_sha256),"ttl_ms":g.lifetime_ms,"plugin_id":plugin,"role":role,"operation":operation}));
        Ok(g.id)
    }
    pub fn claim(&mut self, id: &str) -> Result<Snapshot> {
        if self.failed || self.pin.is_some() {
            return Err("local owner retained".into());
        }
        let live = self
            .live
            .get(id)
            .ok_or("no live approval in original issuer; persistent data cannot grant")?;
        let admission = &live.admission;
        if Instant::now() >= admission.created + Duration::from_millis(admission.spec.ttl_ms) {
            let tx = self
                .db
                .transaction_with_behavior(TransactionBehavior::Immediate)
                .map_err(err)?;
            let mut g = load_grant(&tx, id)?;
            if g.state == 1 {
                g.state = 4;
                save_grant(&tx, &g)?;
            }
            tx.commit().map_err(err)?;
            self.live.remove(id);
            return Err("original approval expired".into());
        }
        let mut pin = Store::open_existing(
            &self.root.join("coordination.sqlite"),
            EventBudget::default(),
        )
        .map_err(err)?;
        pin.pin_service_authority()
            .map_err(|e| format!("owner busy or unavailable: {e}"))?;
        let tx = self
            .db
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(err)?;
        if load_owner(&tx)?.as_ref().is_some_and(|o| !terminal(o)) {
            return Err("owner unconfirmed: no automatic crash takeover".into());
        }
        let mut g = load_grant(&tx, id)?;
        if g != live.record
            || g.issuer != self.issuer
            || g.profile != self.profile.identity
            || g.slot != self.profile.slot
            || g.state != 1
            || g.config_sha256 != admission.config
        {
            return Err("claim binding/state rejected".into());
        }
        g.state = 2;
        save_grant(&tx, &g)?;
        let mut o = proto::Owner {
            version: 1,
            grant_id: id.into(),
            issuer: self.issuer.clone(),
            generation: g.generation,
            phase: "LaunchPending".into(),
            ..Default::default()
        };
        save_owner(&tx, &o)?;
        // Retain the pin even if COMMIT reports an uncertain result. No spawn on that branch.
        self.pin = Some(std::sync::Arc::new(std::sync::Mutex::new(pin)));
        self.active = Some(id.into());
        if let Err(e) = tx.commit() {
            self.failed = true;
            return Err(format!("claim commit unknown; owner retained: {e}"));
        }
        self.event(
            "claim_committed",
            json!({"grant_id":id,"generation":g.generation}),
        );
        let admission = self.live.remove(id).ok_or("live approval lost")?.admission;
        // A second write transaction serializes revoke against the entire sync spawn/register window.
        let tx = self
            .db
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(err)?;
        let latest = load_grant(&tx, id)?;
        let recorded = load_owner(&tx)?.ok_or("missing pending owner")?;
        if latest != g
            || latest.state != 2
            || recorded.grant_id != id
            || recorded.phase != "LaunchPending"
            || Instant::now() >= admission.created + Duration::from_millis(admission.spec.ttl_ms)
        {
            o.phase = "NotStarted".into();
            save_owner(&tx, &o)?;
            tx.commit().map_err(err)?;
            self.pin = None;
            self.active = None;
            return Err("revoked/expired before spawn; never started".into());
        }
        self.host.next_epoch = g.generation;
        let deadline = admission.created + Duration::from_millis(admission.spec.ttl_ms);
        let parent = crate::http_authority::Parent {
            pid: 0,
            session: 0,
            epoch: 0,
            root: self.root.clone(),
            approval: g.clone(),
            store: self.pin.as_ref().unwrap().clone(),
            gate: std::sync::Arc::new(std::sync::Mutex::new(crate::pipe_driver::EffectGate {
                revoked: false,
                deadline,
                ordinal: 0,
                last_write: 0,
                issued_body_end: 0,
                network_pending: false,
            })),
            origin: admission.spec.http_origin.clone(),
            deadline,
        };
        let session = match self.host.launch(admission, parent) {
            Ok(s) => s,
            Err(e) => {
                self.failed = true;
                return Err(format!("launch unconfirmed; owner retained: {e}"));
            }
        };
        let s = session.snapshot();
        o.phase = s.phase.clone();
        o.child_pid = s.pid;
        o.session = s.session;
        o.epoch = s.epoch;
        self.owner_phase = o.phase.clone();
        self.session = Some(session);
        save_owner(&tx, &o)?;
        if let Err(e) = tx.commit() {
            self.failed = true;
            return Err(format!("registration unknown; owner retained: {e}"));
        }
        self.event("owner_registered",json!({"grant_id":id,"generation":g.generation,"pid":s.pid,"session":s.session,"epoch":s.epoch}));
        Ok(s)
    }
    pub async fn approve_http(
        &self,
        proposal_ref: Vec<u8>,
        expected_hash: Vec<u8>,
        response_limit: u32,
    ) -> Result<Value> {
        if self.failed {
            return Err("authority failed closed".into());
        }
        self.session
            .as_ref()
            .ok_or("no live session")?
            .approve_http(proposal_ref, expected_hash, response_limit)
            .await
    }
    pub fn inspect_http(&self) -> Result<Value> {
        Ok(self.session.as_ref().ok_or("no session")?.snapshot().http)
    }
    pub async fn revoke(&mut self, id: &str) -> Result<Value> {
        let tx = self
            .db
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(err)?;
        let mut g = load_grant(&tx, id)?;
        if g.profile != self.profile.identity {
            return Err("foreign profile".into());
        }
        record_revocation(&tx, &mut g, 1, 19)?;
        tx.commit().map_err(err)?;
        self.live.remove(id);
        let local = self.active.as_deref() == Some(id);
        let applied = if local {
            if let Some(s) = &self.session {
                s.revoke().await?;
                true
            } else {
                false
            }
        } else {
            false
        };
        self.event(
            "approval_revoked",
            json!({"grant_id":id,"generation":g.generation,"runtime_applied":applied,"first_revocation_source":g.revocation_source,"first_revocation_reason":g.revocation_reason}),
        );
        Ok(
            json!({"persisted":true,"runtime_applied":applied,"application_pending":!applied,"first_revocation_source":g.revocation_source,"first_revocation_reason":g.revocation_reason}),
        )
    }
    pub async fn stop(&self) -> Result<()> {
        if let Some(s) = &self.session {
            s.stop().await
        } else {
            Ok(())
        }
    }
    pub fn snapshot(&self) -> Option<Snapshot> {
        self.session.as_ref().map(Session::snapshot)
    }
    pub fn inspect(&self, id: Option<&str>) -> Result<Value> {
        let owner = load_owner(&self.db)?;
        let owner=owner.map(|o|json!({"grant_id":o.grant_id,"generation":o.generation,"phase":o.phase,"pid":o.child_pid,"session":o.session,"epoch":o.epoch,"exit_observed":o.exit_observed,"stdout_eof":o.stdout_eof,"stderr_eof":o.stderr_eof,"owner_retained":!terminal(&o),"locally_observed":self.active.as_deref()==Some(&o.grant_id),"automatic_takeover":false}));
        let grant=id.map(|id|{let g=load_grant(&self.db,id)?;Ok::<_,String>(json!({"grant_id":g.id,"issuer":g.issuer,"generation":g.generation,"state":g.state,"config_sha256":wire::hex(&g.config_sha256),"plugin_id":g.plugin_id,"role":g.role,"operation":g.operation,"live_in_this_issuer":self.live.contains_key(id)}))}).transpose()?;
        Ok(
            json!({"profile_id":self.profile.identity,"slot":self.profile.slot,"owner":owner,"grant":grant,"failed_closed":self.failed}),
        )
    }
    pub async fn poll(&mut self) -> Result<bool> {
        let Some(session) = self.session.clone() else {
            return Ok(false);
        };
        let id = self.active.clone().ok_or("active identity missing")?;
        let grant = load_grant(&self.db, &id)?;
        if grant.state == 3 && grant.revocation_source == 1 && session.snapshot().generation == 1 {
            session.revoke().await?;
            self.event("external_revocation_applied", json!({"grant_id":id}));
        }
        let s = session.snapshot();
        if self.failed {
            return Ok(false);
        }
        if s.phase != self.owner_phase {
            let tx = self
                .db
                .transaction_with_behavior(TransactionBehavior::Immediate)
                .map_err(err)?;
            let mut o = load_owner(&tx)?.ok_or("owner missing")?;
            if o.grant_id != id
                || o.issuer != self.issuer
                || o.child_pid != s.pid
                || o.epoch != s.epoch
            {
                self.failed = true;
                return Err("owner binding drift".into());
            }
            o.phase = s.phase.clone();
            o.exit_observed = s.exit_observed;
            o.stdout_eof = s.stdout_eof;
            o.stderr_eof = s.stderr_eof;
            o.snapshot_sha256 = wire::digest(&serde_json::to_vec(&s).unwrap()).to_vec();
            if s.phase == "Released"
                && (!s.exit_observed || !s.stdout_eof || !s.stderr_eof || s.owner_retained)
            {
                self.failed = true;
                return Err("release proof missing".into());
            }
            save_owner(&tx, &o)?;
            if let Err(e) = tx.commit() {
                self.failed = true;
                return Err(format!("owner update unknown; retained: {e}"));
            }
            self.owner_phase = s.phase.clone();
            self.event("owner_phase", json!({"phase":s.phase,"grant_id":id}));
            if s.phase == "Released" {
                self.pin = None;
                self.event(
                    "released",
                    json!({"grant_id":id,"pid":s.pid,"generation":o.generation}),
                );
                return Ok(true);
            }
        }
        Ok(false)
    }
    #[cfg(feature = "qualification-pipe-fault")]
    pub async fn qualification_close_data(
        &self,
        id: &str,
        witness: crate::PartialFrameWitness,
    ) -> Result<Value> {
        if self.failed || self.active.as_deref() != Some(id) || load_grant(&self.db, id)?.state != 2
        {
            return Err("qualification active grant mismatch/revoked".into());
        }
        self.session
            .as_ref()
            .ok_or("qualification live owner missing")?
            .close_data_after_witness(witness)
            .await
    }
}
