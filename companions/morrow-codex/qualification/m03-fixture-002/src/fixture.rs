//! Synthetic-fixture-only barriers and bounded evidence. This is not authority.
//! No filesystem I/O occurs in an Operation cancellation gate or native lock.
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    fs::{File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
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
    CoreRevoke,
    DataPending,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Spec {
    pub version: u32,
    pub mode: Mode,
    pub nonce: String,
    pub request_limit: usize,
    pub response_limit: usize,
    pub consumer_events: usize,
    pub credit_limit: usize,
    pub pipe_buffer: usize,
    pub max_chunk: usize,
    pub min_pending_samples: usize,
    pub min_sample_interval_ms: u64,
    pub revoke_ack_limit_ms: u64,
    pub events_file: String,
    pub release_file: String,
}
fn hex64(s: &str) -> bool {
    s.len() == 64
        && s.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
impl Spec {
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.version != 1
            || !hex64(&self.nonce)
            || self.request_limit != 32768
            || self.response_limit != 65536
            || self.consumer_events != 8
            || self.credit_limit != 16384
            || self.pipe_buffer != 1024
            || self.max_chunk != 1024
            || self.min_pending_samples != 3
            || self.min_sample_interval_ms != 25
            || self.revoke_ack_limit_ms != 500
            || self.events_file != "fixture-events.jsonl"
            || self.release_file != "release-consumer.json"
        {
            return Err("fixture spec outside fixed bounds");
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
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Release {
    version: u32,
    mode: Mode,
    nonce: String,
    spec_sha256: String,
    identity: Identity,
    stage: String,
    event: CoreEventId,
}
struct State {
    identity: Option<Identity>,
    ordinal: u64,
    held: Option<CoreEventId>,
    release_seen: bool,
    queued_a: bool,
    reserved_b: bool,
    suppressed_a: bool,
    suppressed_b: bool,
    paused: bool,
}
struct Inner {
    spec: Spec,
    spec_sha256: String,
    origin: Instant,
    state: Mutex<State>,
    failure: Mutex<Option<&'static str>>,
    failed: CancellationToken,
    released: CancellationToken,
    gate_closed: AtomicBool,
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
    fn release(&self, bytes: &[u8]) -> Result<(), &'static str> {
        let r: Release =
            serde_json::from_slice(bytes).map_err(|_| "invalid fixture release JSON")?;
        let mut s = self.state.lock().unwrap();
        if s.release_seen
            || !self.gate_closed.load(Ordering::Acquire)
            || !s.queued_a
            || !s.reserved_b
            || r.version != 1
            || r.mode != Mode::CoreRevoke
            || self.spec.mode != r.mode
            || r.nonce != self.spec.nonce
            || r.spec_sha256 != self.spec_sha256
            || s.identity.as_ref() != Some(&r.identity)
            || s.held.as_ref() != Some(&r.event)
            || r.stage != "release-consumer"
        {
            return Err("premature, stale or mismatched fixture release");
        }
        s.release_seen = true;
        self.released.cancel();
        Ok(())
    }
}
pub struct Fixture {
    inner: Arc<Inner>,
    tx: mpsc::SyncSender<Value>,
    writer: Mutex<Option<JoinHandle<()>>>,
}
impl Drop for Fixture {
    fn drop(&mut self) {
        self.inner.stop.store(true, Ordering::Release);
    }
}
impl std::fmt::Debug for Fixture {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BoundFixture")
            .field("mode", &self.inner.spec.mode)
            .finish_non_exhaustive()
    }
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
        let release = directory.join(&spec.release_file);
        if release
            .try_exists()
            .map_err(|_| "fixture release existence")?
        {
            return Err("preexisting fixture release");
        }
        let file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(directory.join(&spec.events_file))
            .map_err(|_| "create fixture evidence")?;
        Self::with_writer(spec, expected_sha.into(), file, release)
    }
    fn with_writer(
        spec: Spec,
        digest: String,
        file: File,
        release: PathBuf,
    ) -> Result<Arc<Self>, &'static str> {
        let inner = Arc::new(Inner {
            spec,
            spec_sha256: digest,
            origin: Instant::now(),
            state: Mutex::new(State {
                identity: None,
                ordinal: 0,
                held: None,
                release_seen: false,
                queued_a: false,
                reserved_b: false,
                suppressed_a: false,
                suppressed_b: false,
                paused: false,
            }),
            failure: Mutex::new(None),
            failed: CancellationToken::new(),
            released: CancellationToken::new(),
            gate_closed: AtomicBool::new(false),
            stop: AtomicBool::new(false),
            written: AtomicU64::new(0),
            read_issues: AtomicU64::new(0),
            writer_joined: AtomicBool::new(false),
        });
        let (tx, rx) = mpsc::sync_channel(128);
        let owned = inner.clone();
        let writer = thread::Builder::new()
            .name("m03-fixture-evidence".into())
            .spawn(move || evidence_owner(owned, rx, file, release))
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
        s.identity = Some(identity);
        Ok(())
    }
    pub fn mode(&self) -> Mode {
        self.inner.spec.mode.clone()
    }
    pub fn failed(&self) -> bool {
        self.inner.failed.is_cancelled()
    }
    pub fn fail(&self, reason: &'static str) {
        self.inner.fail(reason)
    }
    pub fn emit(&self, kind: &'static str, detail: Value) {
        let queued = {
            let mut s = self.inner.state.lock().unwrap();
            if s.identity.is_none() {
                drop(s);
                self.fail("unbound fixture record");
                return;
            }
            s.ordinal += 1;
            let record = json!({"identity":s.identity,"mode":self.inner.spec.mode,"nonce":self.inner.spec.nonce,
                "spec_sha256":self.inner.spec_sha256,"ordinal":s.ordinal,"elapsed_ns":self.inner.origin.elapsed().as_nanos(),
                "clock_domain":"guest-fixture-monotonic","kind":kind,"detail":detail});
            // Preserve ordinal order across producers without any blocking send.
            !self.inner.stop.load(Ordering::Acquire) && self.tx.try_send(record).is_ok()
        };
        // Called only outside cancellation/native locks. Never wait for disk.
        if !queued {
            self.fail("fixture evidence queue unavailable");
        }
    }
    pub fn queued(&self, event: &CoreEventId) {
        if event.delta_ordinal == 1 {
            self.inner.state.lock().unwrap().queued_a = true;
        }
        self.emit("core_event_queued", json!({"event":event}));
    }
    pub async fn hold_consumer(&self, event: &CoreEventId, deadline: tokio::time::Instant) -> bool {
        {
            self.inner.state.lock().unwrap().held = Some(event.clone());
        }
        self.emit("core_event_held", json!({"event":event}));
        tokio::select! { biased;
            _=self.inner.failed.cancelled()=>false,
            _=tokio::time::sleep_until(deadline)=>{self.fail("consumer barrier original deadline");false},
            _=self.inner.released.cancelled()=>true,
        }
    }
    pub async fn hold_reserved(
        &self,
        event: &CoreEventId,
        token: CancellationToken,
        deadline: tokio::time::Instant,
    ) -> bool {
        {
            self.inner.state.lock().unwrap().reserved_b = true;
        }
        self.emit("core_event_reserved", json!({"event":event}));
        tokio::select! {biased;
            _=self.inner.failed.cancelled()=>false,
            _=tokio::time::sleep_until(deadline)=>{self.fail("reserved barrier original deadline");false},
            _=token.cancelled()=>true,
        }
    }
    pub fn suppressed(&self, event: &CoreEventId, reserved: bool, gate_closed: bool) {
        if !gate_closed {
            self.fail("suppression without cancellation gate");
        }
        {
            let mut s = self.inner.state.lock().unwrap();
            if reserved {
                s.suppressed_b = true;
            } else {
                s.suppressed_a = true;
            }
        }
        self.emit(
            if reserved {
                "core_reserved_suppressed"
            } else {
                "core_event_suppressed"
            },
            json!({"event":event,"gate_closed":gate_closed,"permit_released":reserved}),
        );
    }
    pub fn observe_revoke(&self, received: Value, gate_closed: bool) {
        if self.inner.gate_closed.load(Ordering::Acquire) {
            return;
        }
        self.emit("host_revoke_received", received);
        if gate_closed {
            self.inner.gate_closed.store(true, Ordering::Release);
            self.emit(
                "gate_closed",
                json!({"read_issue_count":self.inner.read_issues.load(Ordering::Acquire)}),
            );
        }
    }
    pub fn read_issued(&self, count: u64) {
        self.inner.read_issues.store(count, Ordering::Release);
    }
    pub fn pause_reader(&self, last_read_id: u64, count: u64) {
        self.inner.state.lock().unwrap().paused = true;
        self.emit(
            "data_read_paused",
            json!({"last_read_id":last_read_id,"read_issue_count":count,
            "framer_bytes":0,"framer_expected":4,"read_operation_present":false}),
        );
    }
    pub fn request_stop(&self) {
        self.inner.stop.store(true, Ordering::Release);
    }
    pub async fn join_until(&self, deadline: tokio::time::Instant) -> bool {
        loop {
            if tokio::time::Instant::now() >= deadline {
                return self.inner.writer_joined.load(Ordering::Acquire);
            }
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
                return false;
            }
            tokio::time::sleep(Duration::from_millis(2)).await;
        }
    }
    pub fn snapshot(&self) -> Value {
        let s = self.inner.state.lock().unwrap();
        let evidence_ok = !self.failed() && self.inner.written.load(Ordering::Acquire) == s.ordinal;
        let stages = match self.inner.spec.mode {
            Mode::CoreRevoke => {
                s.queued_a
                    && s.held.is_some()
                    && s.reserved_b
                    && s.release_seen
                    && s.suppressed_a
                    && s.suppressed_b
            }
            Mode::DataPending => s.paused,
        };
        json!({"mode":self.inner.spec.mode,"spec_sha256":self.inner.spec_sha256,"nonce":self.inner.spec.nonce,
            "failure":*self.inner.failure.lock().unwrap(),"enqueued_records":s.ordinal,"written_records":self.inner.written.load(Ordering::Acquire),
            "evidence_complete":evidence_ok,"stages_complete":stages,"gate_closed":self.inner.gate_closed.load(Ordering::Acquire),
            "read_issue_count":self.inner.read_issues.load(Ordering::Acquire),"writer_joined":self.inner.writer_joined.load(Ordering::Acquire),
            "runtime_qualified":false})
    }
}
#[cfg(test)]
#[path = "fixture_tests.rs"]
pub(crate) mod tests;

fn evidence_owner(inner: Arc<Inner>, rx: mpsc::Receiver<Value>, mut file: File, release: PathBuf) {
    let mut release_read = false;
    loop {
        match rx.recv_timeout(Duration::from_millis(5)) {
            Ok(record) => {
                if serde_json::to_writer(&mut file, &record).is_err()
                    || file.write_all(b"\n").and_then(|_| file.flush()).is_err()
                {
                    inner.fail("fixture evidence write");
                } else {
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
        if !release_read && !inner.stop.load(Ordering::Acquire) {
            match release.try_exists() {
                Ok(true) => {
                    release_read = true;
                    match bounded_read(&release, 2048).and_then(|b| inner.release(&b)) {
                        Ok(()) => {}
                        Err(e) => inner.fail(e),
                    }
                }
                Ok(false) => {}
                Err(_) => inner.fail("fixture release existence"),
            }
        }
    }
}
