//! Passive fixture diagnostics only. No barrier, release, authority or fake event.
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeSet,
    fs::{File, OpenOptions},
    io::{Read, Write},
    path::Path,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc,
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};
use tokio_util::sync::CancellationToken;

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum Mode {
    PassiveObserve,
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum Scenario {
    AuthorityDeadline,
    NetworkAbort,
    PipePartialClose,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Spec {
    pub version: u32,
    pub mode: Mode,
    pub scenario: Scenario,
    pub nonce: String,
    pub request_limit: usize,
    pub response_limit: usize,
    pub consumer_events: usize,
    pub credit_limit: usize,
    pub pipe_buffer: usize,
    pub max_chunk: usize,
    pub deadline_policy: String,
    pub events_file: String,
    pub evidence_queue_capacity: usize,
    pub evidence_record_limit: usize,
    pub evidence_record_limit_bytes: usize,
    pub evidence_file_limit_bytes: usize,
    #[serde(deserialize_with = "required_nullable_usize")]
    pub pipe_prefix_bytes: Option<usize>,
}
fn required_nullable_usize<'de, D: serde::Deserializer<'de>>(
    d: D,
) -> Result<Option<usize>, D::Error> {
    Option::<usize>::deserialize(d)
}
fn hex64(s: &str) -> bool {
    s.len() == 64
        && s.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
impl Spec {
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.version != 2
            || !hex64(&self.nonce)
            || self.nonce == "0".repeat(64)
            || self.request_limit != 32768
            || self.response_limit != 65536
            || self.consumer_events != 8
            || self.credit_limit != 16384
            || self.pipe_buffer != 1024
            || self.max_chunk != 1024
            || self.deadline_policy != "retain-original"
            || self.events_file != "fixture-events.jsonl"
            || self.evidence_queue_capacity != 128
            || self.evidence_record_limit != 128
            || self.evidence_record_limit_bytes != 8192
            || self.evidence_file_limit_bytes != 1048576
            || self.pipe_prefix_bytes
                != if self.scenario == Scenario::PipePartialClose {
                    Some(12)
                } else {
                    None
                }
        {
            return Err("passive fixture spec outside fixed bounds");
        }
        Ok(())
    }
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Identity {
    pub session: u64,
    pub epoch: u64,
    pub child_pid: u32,
    pub attempt: u64,
    pub operation_id_sha256: String,
    pub host_execution_config_sha256: String,
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct CoreEventId {
    pub stream_ordinal: u64,
    pub delta_ordinal: u64,
    pub bytes: usize,
    pub sha256: String,
    pub kind: String,
}
impl CoreEventId {
    pub fn delta(stream_ordinal: u64, delta_ordinal: u64, text: &str) -> Self {
        Self {
            stream_ordinal,
            delta_ordinal,
            bytes: text.len(),
            sha256: format!("{:x}", Sha256::digest(text.as_bytes())),
            kind: "OutputTextDelta".into(),
        }
    }
    pub fn other(stream_ordinal: u64, delta_ordinal: u64, kind: &str) -> Self {
        Self {
            stream_ordinal,
            delta_ordinal,
            bytes: 0,
            sha256: format!("{:x}", Sha256::digest([])),
            kind: kind.into(),
        }
    }
}
struct State {
    identity: Option<Identity>,
    ordinal: u64,
    observed: BTreeSet<&'static str>,
    authority: Option<Value>,
}
struct Inner {
    spec: Spec,
    spec_sha256: String,
    origin: Instant,
    state: Mutex<State>,
    failure: Mutex<Option<&'static str>>,
    failed: CancellationToken,
    stop: AtomicBool,
    written: AtomicU64,
    read_issues: AtomicU64,
    writer_joined: AtomicBool,
}
impl Inner {
    fn fail(&self, reason: &'static str) {
        self.failure.lock().unwrap().get_or_insert(reason);
        self.failed.cancel();
    }
}
pub struct Fixture {
    inner: Arc<Inner>,
    tx: mpsc::SyncSender<Value>,
    writer: Mutex<Option<JoinHandle<()>>>,
}
fn bounded_read(path: &Path, max: u64) -> Result<Vec<u8>, &'static str> {
    let metadata = std::fs::symlink_metadata(path).map_err(|_| "fixture file metadata")?;
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if metadata.file_attributes() & 0x400 != 0 {
            return Err("reparse fixture file");
        }
    }
    if !metadata.is_file() || metadata.len() > max {
        return Err("fixture file bound/type");
    }
    let mut bytes = Vec::new();
    File::open(path)
        .map_err(|_| "fixture file open")?
        .take(max + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| "fixture file read")?;
    if bytes.len() as u64 > max {
        return Err("fixture read bound");
    }
    Ok(bytes)
}
impl Fixture {
    pub fn open(directory: &Path, expected_sha: &str) -> Result<Arc<Self>, &'static str> {
        if !hex64(expected_sha) {
            return Err("fixture spec digest format");
        }
        let bytes = bounded_read(&directory.join("fixture-spec.json"), 4096)?;
        if format!("{:x}", Sha256::digest(&bytes)) != expected_sha {
            return Err("fixture spec digest mismatch");
        }
        let spec: Spec = serde_json::from_slice(&bytes).map_err(|_| "fixture spec JSON")?;
        spec.validate()?;
        let file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(directory.join(&spec.events_file))
            .map_err(|_| "create fixture evidence")?;
        Self::with_writer(spec, expected_sha.into(), file)
    }
    fn with_writer(spec: Spec, digest: String, file: File) -> Result<Arc<Self>, &'static str> {
        let inner = Arc::new(Inner {
            spec,
            spec_sha256: digest,
            origin: Instant::now(),
            state: Mutex::new(State {
                identity: None,
                ordinal: 0,
                observed: BTreeSet::new(),
                authority: None,
            }),
            failure: Mutex::new(None),
            failed: CancellationToken::new(),
            stop: AtomicBool::new(false),
            written: AtomicU64::new(0),
            read_issues: AtomicU64::new(0),
            writer_joined: AtomicBool::new(false),
        });
        let (tx, rx) = mpsc::sync_channel(128);
        let owned = inner.clone();
        let writer = thread::Builder::new()
            .name("m03-passive-evidence".into())
            .spawn(move || evidence_owner(owned, rx, file))
            .map_err(|_| "fixture evidence thread")?;
        Ok(Arc::new(Self {
            inner,
            tx,
            writer: Mutex::new(Some(writer)),
        }))
    }
    pub fn bind(&self, identity: Identity) -> Result<(), &'static str> {
        let mut s = self.inner.state.lock().unwrap();
        if s.identity.is_some() {
            return Err("fixture identity already bound");
        }
        if identity.session == 0
            || identity.epoch == 0
            || identity.child_pid == 0
            || identity.attempt == 0
            || !hex64(&identity.operation_id_sha256)
            || !hex64(&identity.host_execution_config_sha256)
        {
            return Err("fixture identity bounds");
        }
        s.identity = Some(identity);
        Ok(())
    }
    pub fn bind_authority(
        &self,
        first_read: Instant,
        deadline: Instant,
        remaining_ms: u64,
    ) -> Result<(), &'static str> {
        let first = first_read
            .checked_duration_since(self.inner.origin)
            .ok_or("authority read clock origin")?;
        let expiry = deadline
            .checked_duration_since(self.inner.origin)
            .ok_or("authority deadline clock origin")?;
        let detail = json!({"initial_remaining_ms":remaining_ms,"clock_domain":"guest-fixture-monotonic",
            "first_read_offset_ns":u64::try_from(first.as_nanos()).map_err(|_|"read offset overflow")?,
            "original_deadline_offset_ns":u64::try_from(expiry.as_nanos()).map_err(|_|"deadline offset overflow")?,
            "deadline_recomputed_after_binding":false});
        {
            let mut s = self.inner.state.lock().unwrap();
            if s.authority.is_some() {
                return Err("fixture authority already bound");
            }
            s.authority = Some(detail.clone());
        }
        self.emit_once("authority_bound", detail);
        Ok(())
    }
    pub fn authority_observation(&self) -> Value {
        self.inner
            .state
            .lock()
            .unwrap()
            .authority
            .clone()
            .unwrap_or(Value::Null)
    }
    pub fn failed(&self) -> bool {
        self.inner.failed.is_cancelled()
    }
    pub fn fail(&self, reason: &'static str) {
        self.inner.fail(reason)
    }
    pub fn emit(&self, kind: &'static str, detail: Value) {
        self.emit_impl(kind, detail, false)
    }
    pub fn emit_once(&self, kind: &'static str, detail: Value) {
        self.emit_impl(kind, detail, true)
    }
    fn emit_impl(&self, kind: &'static str, detail: Value, once: bool) {
        let mut s = self.inner.state.lock().unwrap();
        if once && s.observed.contains(kind) {
            return;
        }
        if self.inner.stop.load(Ordering::Acquire) {
            self.fail("marker after evidence stop");
            return;
        }
        if s.identity.is_none() {
            self.fail("unbound fixture record");
            return;
        }
        if s.ordinal >= 128 {
            self.fail("fixture total record limit");
            return;
        }
        let ordinal = s.ordinal + 1;
        let record = json!({"identity":s.identity,"mode":self.inner.spec.mode,"scenario":self.inner.spec.scenario,
            "nonce":self.inner.spec.nonce,"spec_sha256":self.inner.spec_sha256,"ordinal":ordinal,
            "elapsed_ns":self.inner.origin.elapsed().as_nanos(),"clock_domain":"guest-fixture-monotonic","kind":kind,"detail":detail});
        if serde_json::to_vec(&record).map_or(true, |b| b.len() > 8192) {
            self.fail("fixture record byte limit");
            return;
        }
        if self.tx.try_send(record).is_err() {
            self.fail("fixture evidence queue overflow/disconnected");
            return;
        }
        s.ordinal = ordinal;
        s.observed.insert(kind);
    }
    pub fn observe_revoke(&self, detail: Value, gate_closed: bool) {
        self.emit("host_revoke_received", detail);
        if gate_closed {
            self.emit_once(
                "gate_closed",
                json!({"read_issue_count":self.inner.read_issues.load(Ordering::Acquire)}),
            );
        }
    }
    pub fn read_issued(&self, count: u64) {
        self.inner.read_issues.store(count, Ordering::Release);
    }
    pub fn request_stop(&self) {
        let _s = self.inner.state.lock().unwrap();
        self.inner.stop.store(true, Ordering::Release);
    }
    pub async fn join_until(&self, deadline: tokio::time::Instant) -> bool {
        loop {
            {
                let mut slot = self.writer.lock().unwrap();
                if slot.as_ref().is_some_and(|h| h.is_finished()) {
                    let ok = slot.take().unwrap().join().is_ok();
                    self.inner.writer_joined.store(ok, Ordering::Release);
                    if !ok {
                        self.fail("fixture writer panic");
                    }
                    return ok;
                }
                if slot.is_none() {
                    return self.inner.writer_joined.load(Ordering::Acquire);
                }
            }
            if tokio::time::Instant::now() >= deadline {
                return self.inner.writer_joined.load(Ordering::Acquire);
            }
            tokio::time::sleep(Duration::from_millis(2)).await;
        }
    }
    pub fn snapshot(&self) -> Value {
        let s = self.inner.state.lock().unwrap();
        let written = self.inner.written.load(Ordering::Acquire);
        json!({"mode":self.inner.spec.mode,"scenario":self.inner.spec.scenario,"nonce":self.inner.spec.nonce,
            "spec_sha256":self.inner.spec_sha256,"failure":*self.inner.failure.lock().unwrap(),
            "enqueued_records":s.ordinal,"written_records":written,"evidence_complete":!self.failed()&&written==s.ordinal,
            "observed_stages":s.observed,"read_issue_count":self.inner.read_issues.load(Ordering::Acquire),
            "writer_joined":self.inner.writer_joined.load(Ordering::Acquire),"runtime_qualified":false,"product_accepted":false})
    }
}
fn evidence_owner(inner: Arc<Inner>, rx: mpsc::Receiver<Value>, mut file: File) {
    let mut written_bytes = 0usize;
    loop {
        match rx.recv_timeout(Duration::from_millis(5)) {
            Ok(record) => {
                let mut bytes = match serde_json::to_vec(&record) {
                    Ok(b) => b,
                    Err(_) => {
                        inner.fail("fixture record serialization");
                        continue;
                    }
                };
                bytes.push(b'\n');
                if written_bytes.saturating_add(bytes.len()) > 1048576 {
                    inner.fail("fixture file byte limit");
                    continue;
                }
                if file.write_all(&bytes).and_then(|_| file.flush()).is_err() {
                    inner.fail("fixture evidence write");
                } else {
                    written_bytes += bytes.len();
                    inner.written.fetch_add(1, Ordering::Release);
                }
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
            Err(mpsc::RecvTimeoutError::Timeout) => {
                if inner.stop.load(Ordering::Acquire) {
                    break;
                }
            }
        }
    }
}
#[cfg(test)]
#[path = "fixture_tests.rs"]
pub(crate) mod tests;
