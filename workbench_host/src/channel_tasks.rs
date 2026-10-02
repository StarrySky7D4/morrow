//! Windows local-memory channel jobs on the original workbench owner.
//! Uploaded bytes are caller-provided data, not authority to read paths, content or network.
use crate::{Result, Workbench, WorkbenchState, io_tasks::AccessError, now};
use morrow_core::{
    channel::{Budget, Kind, Status},
    task::{Invocation, Transform},
};
use morrow_plugin_runtime::{
    channel::{
        ChannelBroker, ChannelCleanup, CleanupProof, ProducerOutcome, Snapshot as RuntimeSnapshot,
        Source,
    },
    manager::ManagedInstance,
    package::TaskReport,
};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeSet,
    panic::{AssertUnwindSafe, catch_unwind},
    sync::{Arc, Mutex},
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

pub const MAX_SOURCE_FRAMES: u32 = 64;
pub const MAX_JOB_BYTES: u64 = 1024 * 1024;
const MAX_HOST_JOBS: usize = 64;
const MAX_HOST_RESERVED_BYTES: u64 = 8 * MAX_JOB_BYTES;
const MAX_HOST_SOURCE_FRAMES: u32 = 256;
const MAX_HOST_REQUESTS: u64 = 4096;
pub const DIRECTORY_INPUT_TYPE: &str = crate::channel_directory_preflight::DIRECTORY_INPUT_TYPE;
pub struct Prepare {
    pub submission: [u8; 32],
    pub package_id: String,
    pub package_digest: [u8; 32],
    pub registry_revision: u64,
    pub handler: String,
    pub kind: Kind,
    pub duplex: bool,
    pub budget: Budget,
    pub lifetime_ms: u32,
    pub frame_count: u32,
    pub total_bytes: u64,
}
#[derive(Clone)]
pub struct SourceFrame {
    pub sequence: u64,
    pub bytes: Vec<u8>,
    pub cursor: Vec<u8>,
}
#[derive(Clone, Debug)]
pub struct State {
    pub key: [u8; 32],
    pub submission: [u8; 32],
    pub directory: Vec<u8>,
    pub reference: [u8; 32],
    pub source_epoch: [u8; 32],
    pub phase: u16,
    pub status: Status,
    pub last_acked: u64,
    pub accepted_sequence: u64,
    pub observed_sequence: u64,
    pub cleanup_proof: CleanupProof,
    pub producer_outcome: ProducerOutcome,
    pub task_state: u16,
    pub task_error: String,
    pub output_type: String,
    pub output: Vec<u8>,
    pub input_sha256: Vec<u8>,
    pub uploaded_frames: u32,
    pub uploaded_bytes: u64,
    pub source_frames: u32,
    pub source_bytes: u64,
    pub resource_reclaimed: bool,
    pub close_requested: bool,
    pub observed_bytes: u64,
    pub observed_sha256: Vec<u8>,
    pub worker_joined: bool,
    pub snapshot_pending: bool,
}
pub struct Sent {
    pub present: bool,
    pub sequence: u64,
    pub bytes: Vec<u8>,
    pub offset: u32,
    pub total_bytes: u32,
    pub bytes_sha256: Vec<u8>,
}
#[derive(Default)]
struct Progress {
    source_frames: u32,
    source_bytes: u64,
    sent: Vec<(u64, Vec<u8>)>,
    observed_bytes: u64,
    observed_hash: Sha256,
}
#[derive(Clone, Default)]
struct ProgressView {
    source_frames: u32,
    source_bytes: u64,
    observed_sequence: u64,
    observed_bytes: u64,
    observed_sha256: Vec<u8>,
}
impl Progress {
    fn view(&self) -> ProgressView {
        ProgressView {
            source_frames: self.source_frames,
            source_bytes: self.source_bytes,
            observed_sequence: self.sent.last().map_or(0, |(sequence, _)| *sequence),
            observed_bytes: self.observed_bytes,
            observed_sha256: if self.sent.is_empty() {
                vec![]
            } else {
                self.observed_hash.clone().finalize().to_vec()
            },
        }
    }
}
struct Exit {
    owner: WorkbenchState,
    report: Option<TaskReport>,
    error: String,
    repair: bool,
}
struct StopFence {
    package_id: String,
    digest: [u8; 32],
    revision: u64,
}
struct Job {
    key: [u8; 32],
    request: Prepare,
    deadline: Instant,
    directory: Vec<u8>,
    instance: Arc<ManagedInstance>,
    broker: Arc<ChannelBroker>,
    cleanup: Arc<ChannelCleanup>,
    input_type: String,
    output_type: String,
    max_input_bytes: u32,
    max_output_bytes: u32,
    uploaded: Vec<SourceFrame>,
    uploaded_bytes: u64,
    run_admitted: bool,
    worker: Option<JoinHandle<Exit>>,
    worker_started: bool,
    worker_joined: bool,
    report: Option<TaskReport>,
    error: String,
    close_requested: bool,
    disconnected: bool,
    input_sha256: Vec<u8>,
    progress: Arc<Mutex<Progress>>,
    last_progress: ProgressView,
    last_snapshot: Option<RuntimeSnapshot>,
    snapshot_pending: bool,
}
#[derive(Default)]
pub(crate) struct ChannelTasks {
    owner_ready: bool,
    job: Option<Job>,
    seen: BTreeSet<[u8; 32]>,
    requests: u64,
    reserved_bytes: u64,
    source_frames: u32,
    fences: Vec<StopFence>,
}
fn short_error(value: impl std::fmt::Display) -> String {
    let mut text = value.to_string();
    while text.len() > 256 {
        text.pop();
    }
    text
}
impl ChannelTasks {
    fn touch(&mut self, cleanup_only: bool) -> Result<()> {
        if self.requests >= MAX_HOST_REQUESTS {
            if cleanup_only {
                return Ok(());
            }
            return Err("local channel host request budget exhausted".into());
        }
        self.requests += 1;
        Ok(())
    }
    fn checked(&mut self, key: &[u8]) -> Result<&mut Job> {
        self.job
            .as_mut()
            .filter(|job| key == job.key)
            .ok_or_else(|| "stale local channel job key".into())
    }
    pub(crate) fn request_stop(&mut self) {
        if let Some(job) = &mut self.job {
            job.stop();
        }
    }
    pub(crate) fn stop_package(&mut self, id: &str) {
        if let Some(job) = &mut self.job {
            if job.request.package_id == id {
                job.stop();
            }
        }
    }
    pub(crate) fn fence_catalog_stop(
        &mut self,
        id: &str,
        digest: &[u8],
        revision: u64,
    ) -> Result<()> {
        let Some(job) = &mut self.job else {
            return Ok(());
        };
        if job.request.package_id != id || job.disconnected {
            return Ok(());
        }
        if digest != job.request.package_digest || revision != job.request.registry_revision {
            return Err("stale catalog decision cannot revoke another channel grant".into());
        }
        if !self.fences.iter().any(|fence| fence.package_id == id) {
            self.fences.push(StopFence {
                package_id: id.into(),
                digest: job.request.package_digest,
                revision,
            });
        }
        job.stop();
        Ok(())
    }
    pub(crate) fn clear_catalog_fence(&mut self, id: &str, digest: &[u8], revision: u64) {
        self.fences.retain(|fence| {
            !(fence.package_id == id && digest == fence.digest && revision >= fence.revision)
        });
    }
}
impl Job {
    fn stop(&mut self) {
        self.close_requested = true;
        self.instance.request_stop(); // original authority, without waiting for an ACK lock
        let _ = self.cleanup.try_cleanup();
    }
    fn ready(&self) -> bool {
        self.uploaded.len() == self.request.frame_count as usize
            && self.uploaded_bytes == self.request.total_bytes
    }
    fn observe(&mut self) {
        let _ = self.cleanup.try_reap();
        let snapshot = self.broker.try_snapshot();
        let progress = self
            .progress
            .try_lock()
            .ok()
            .map(|progress| progress.view());
        match (snapshot, progress) {
            (Some(snapshot), Some(progress))
                if snapshot.last_acked <= u64::from(progress.source_frames)
                    && u64::from(progress.source_frames)
                        <= snapshot.last_acked.saturating_add(1)
                    && progress.observed_sequence <= snapshot.accepted_sequence
                    && u64::from(progress.source_frames)
                        .saturating_add(progress.observed_sequence)
                        <= snapshot.usage.messages
                    && progress
                        .source_bytes
                        .saturating_add(progress.observed_bytes)
                        <= snapshot.usage.bytes =>
            {
                self.last_snapshot = Some(snapshot);
                self.last_progress = progress;
                self.snapshot_pending = false;
            }
            _ => self.snapshot_pending = true,
        }
    }
    fn state(&self, gate_closed: bool) -> State {
        let expired = Instant::now() >= self.deadline;
        let snapshot = self.last_snapshot;
        let reclaimed = snapshot.is_some_and(|snapshot| snapshot.resource_reclaimed);
        let mut task_state = 0;
        let mut task_error = self.error.clone();
        let mut output = vec![];
        if let Some(report) = &self.report {
            if let Some(failure) = &report.failure {
                task_state = 2;
                task_error = short_error(format!("guest failure: {failure:?}"));
            } else if report.execution.outcome.is_err() {
                task_state = 3;
                task_error = short_error(format!("runtime: {:?}", report.execution.outcome));
            } else if let Some(value) = &report.output {
                task_state = 1;
                output = value.bytes.clone();
            } else {
                task_state = 4;
                task_error = "completed worker has no validated task output".into();
            }
        } else if !self.error.is_empty() {
            task_state = 4;
        }
        if !self.error.is_empty() {
            task_state = 4;
            output.clear();
        }
        // A diagnostic response never revives delivery after caller close, deadline or owner loss.
        if gate_closed || expired || self.close_requested {
            output.clear();
            if self.run_admitted && task_state != 0 {
                task_state = 4;
            }
            if task_error.is_empty() {
                task_error = "original channel delivery gate closed; no automatic replay".into();
            }
        }
        let phase = if self.close_requested {
            if (!self.worker_started || self.worker_joined) && reclaimed && self.disconnected {
                5
            } else {
                4
            }
        } else if self.worker_joined {
            3
        } else if self.run_admitted {
            2
        } else if self.ready() {
            1
        } else {
            0
        };
        let status = if gate_closed {
            Status::Unknown
        } else if expired {
            Status::Expired
        } else if self.close_requested && !reclaimed {
            Status::ClosingUnconfirmed
        } else {
            snapshot
                .and_then(|snapshot| snapshot.terminal_cause)
                .unwrap_or(Status::Ready)
        };
        let endpoint = self.broker.endpoint();
        State {
            key: self.key,
            submission: self.request.submission,
            directory: self.directory.clone(),
            reference: endpoint.reference,
            source_epoch: endpoint.source_epoch,
            phase,
            status,
            last_acked: snapshot.map_or(0, |snapshot| snapshot.last_acked),
            accepted_sequence: snapshot.map_or(0, |snapshot| snapshot.accepted_sequence),
            observed_sequence: self.last_progress.observed_sequence,
            cleanup_proof: snapshot
                .map_or(CleanupProof::Pending, |snapshot| snapshot.cleanup_proof),
            producer_outcome: snapshot.map_or(ProducerOutcome::Pending, |snapshot| {
                snapshot.producer_outcome
            }),
            task_state,
            task_error,
            output_type: self.output_type.clone(),
            output,
            input_sha256: self.input_sha256.clone(),
            uploaded_frames: self.uploaded.len() as u32,
            uploaded_bytes: self.uploaded_bytes,
            source_frames: self.last_progress.source_frames,
            source_bytes: self.last_progress.source_bytes,
            resource_reclaimed: reclaimed,
            close_requested: self.close_requested,
            observed_bytes: self.last_progress.observed_bytes,
            observed_sha256: self.last_progress.observed_sha256.clone(),
            worker_joined: self.worker_joined,
            snapshot_pending: self.snapshot_pending,
        }
    }
}
impl Workbench {
    /// Called only by the Windows entry after its existing supervised-owner proof and watch bind.
    pub fn bind_supervised_channel_owner(&mut self) -> Result<()> {
        self.product_gate().check()?;
        self.state.local_mut()?.local_channel_owner_ready = true;
        self.channel_tasks.owner_ready = true;
        Ok(())
    }
    fn channel_reclaim(&mut self) -> Result<()> {
        let Some(job) = &mut self.channel_tasks.job else {
            return Ok(());
        };
        if job.worker.as_ref().is_some_and(JoinHandle::is_finished) {
            let joined = job
                .worker
                .take()
                .expect("finished original executor")
                .join();
            job.worker_joined = true;
            match joined {
                Ok(exit) => {
                    self.state.restore_channel_owner(exit.owner, exit.repair)?;
                    job.report = exit.report;
                    job.error = exit.error;
                }
                Err(_) => {
                    job.stop();
                    job.error =
                        "original executor panicked; owner unavailable; outcome Unknown".into();
                    self.state.channel_owner_lost();
                    return Err(AccessError::OwnerUnavailable.into());
                }
            }
        }
        if Instant::now() >= job.deadline {
            job.stop();
        }
        if let Ok(owner) = self.state.local() {
            if owner
                .manager
                .as_ref()
                .is_none_or(|manager| manager.revision() != job.request.registry_revision)
            {
                job.stop();
            }
        }
        job.observe();
        if job.close_requested
            && (!job.worker_started || job.worker_joined)
            && job
                .last_snapshot
                .is_some_and(|snapshot| snapshot.resource_reclaimed)
            && !job.disconnected
        {
            let owner = self.state.local_mut()?;
            owner.host.disconnect(job.instance.connection())?;
            job.disconnected = true;
            if let Some(manager) = &mut owner.manager {
                manager.reap_channels();
            }
        }
        Ok(())
    }
    pub fn prepare_channel(&mut self, mut request: Prepare) -> Result<State> {
        self.channel_tasks.touch(false)?;
        self.product_gate().check()?;
        if !self.channel_tasks.owner_ready {
            return Err("local channels require the current Windows supervised owner".into());
        }
        self.channel_reclaim()?;
        if let Some(old) = &self.channel_tasks.job {
            if !old.disconnected || old.worker.is_some() {
                return Err("close and reclaim the original channel job first".into());
            }
        }
        if request.submission == [0; 32]
            || request.package_digest == [0; 32]
            || self.channel_tasks.seen.contains(&request.submission)
            || self.channel_tasks.seen.len() >= MAX_HOST_JOBS
            || request.lifetime_ms == 0
            || request.frame_count > MAX_SOURCE_FRAMES
            || request.total_bytes > MAX_JOB_BYTES
            || (request.frame_count == 0) != (request.total_bytes == 0)
            || request.frame_count == 0 && !request.duplex
            || request.duplex && request.frame_count != 0
        {
            return Err("invalid, repeated or exhausted local source preparation".into());
        }
        if self
            .channel_tasks
            .fences
            .iter()
            .any(|fence| fence.package_id == request.package_id)
        {
            return Err(
                "apply the pending catalog stop or explicitly reapprove before a new source grant"
                    .into(),
            );
        }
        request.budget.validate()?;
        let state = self.local_state()?;
        let manager = state.manager.as_ref().ok_or("plugin manager unavailable")?;
        let selection = manager
            .selection(&request.package_id)
            .ok_or("plugin is not selected")?;
        if manager.revision() != request.registry_revision
            || selection.digest != request.package_digest
            || !selection.enabled
        {
            return Err("local channel package selection changed".into());
        }
        let package = manager.installed_package(request.package_digest)?;
        if !package.capabilities().is_empty()
            || package.io_declaration().is_some()
            || !package.manifest().dependencies.is_empty()
        {
            return Err("local channel route accepts source-only packages".into());
        }
        let declaration = package
            .channel_declaration()
            .ok_or("channel profile is not declared")?;
        if !declaration.handlers.contains(&request.handler) {
            return Err("channel handler is not declared".into());
        }
        let declared_kind = match request.kind {
            Kind::ByteStream => 1,
            Kind::Events => 2,
        };
        if !declaration.kinds.contains(&declared_kind) {
            return Err("channel source kind is not declared".into());
        }
        let metadata = package
            .manifest()
            .transform_handlers
            .iter()
            .find(|handler| handler.handler == request.handler)
            .ok_or("channel type metadata is missing")?;
        let input_type = metadata.input_type.clone();
        let output_type = metadata.output_type.clone();
        let max_input_bytes = metadata.max_input_bytes.min(64 * 1024);
        let max_output_bytes = metadata.max_output_bytes.min(64 * 1024);
        let ceiling = Budget::from_proto(
            declaration
                .budget
                .as_ref()
                .ok_or("channel budget is missing")?,
        )?;
        request.budget = Budget {
            max_channels: 1,
            max_frame_bytes: request.budget.max_frame_bytes.min(ceiling.max_frame_bytes),
            max_bytes: request
                .budget
                .max_bytes
                .min(ceiling.max_bytes)
                .min(MAX_JOB_BYTES),
            max_messages: request
                .budget
                .max_messages
                .min(ceiling.max_messages)
                .min(128),
            max_requests: request
                .budget
                .max_requests
                .min(ceiling.max_requests)
                .min(512),
            max_duration_ms: request
                .budget
                .max_duration_ms
                .min(ceiling.max_duration_ms)
                .min(30_000),
        };
        request.budget.validate()?;
        if u64::from(request.lifetime_ms) > request.budget.max_duration_ms
            || u64::from(request.frame_count) > request.budget.max_messages
            || request.total_bytes > request.budget.max_bytes
        {
            return Err("local source exceeds the approved original grant".into());
        }
        crate::channel_directory_preflight::preflight_directory_input(
            &input_type,
            max_input_bytes,
            request.kind,
            request.budget,
        )?;
        let reserve_bytes = self
            .channel_tasks
            .reserved_bytes
            .checked_add(request.budget.max_bytes)
            .ok_or("host byte budget overflow")?;
        let reserve_frames = self
            .channel_tasks
            .source_frames
            .checked_add(request.frame_count)
            .ok_or("host frame budget overflow")?;
        if reserve_bytes > MAX_HOST_RESERVED_BYTES || reserve_frames > MAX_HOST_SOURCE_FRAMES {
            return Err("local channel host cumulative budget exhausted".into());
        }
        let mut key = [0; 32];
        getrandom::fill(&mut key)?;
        let start = state.start;
        let tick = now(start);
        let expires = tick
            .checked_add(u64::from(request.lifetime_ms))
            .ok_or("channel deadline overflow")?;
        let deadline = Instant::now()
            .checked_add(Duration::from_millis(u64::from(request.lifetime_ms)))
            .ok_or("channel deadline overflow")?;
        let (instance, broker) = {
            let state = self.local_state_mut()?;
            state.host.prepare_write()?;
            let manager = state.manager.as_mut().ok_or("plugin manager unavailable")?;
            let instance = Arc::new(manager.connect(&request.package_id, &mut state.host)?);
            let bound = manager.bind_channel(
                &state.host,
                &instance,
                request.package_digest,
                request.registry_revision,
                Source {
                    kind: request.kind,
                    duplex: request.duplex,
                    checkpoint_scope: if request.kind == Kind::Events {
                        Some(key)
                    } else {
                        None
                    },
                },
                request.budget,
                expires,
                tick,
            );
            let broker = match bound {
                Ok(broker) => Arc::new(broker),
                Err(error) => {
                    instance.request_stop();
                    let _ = state.host.disconnect(instance.connection());
                    return Err(error.into());
                }
            };
            (instance, broker)
        };
        let directory = broker.directory().encode()?;
        let cleanup = Arc::new(broker.cleanup_handle());
        let stop_instance = instance.clone();
        if let Err(error) = self
            .product_gate()
            .register(Arc::new(move || stop_instance.request_stop()))
        {
            instance.request_stop();
            let _ = self
                .state
                .local_mut()?
                .host
                .disconnect(instance.connection());
            return Err(error);
        }
        self.channel_tasks.seen.insert(request.submission);
        self.channel_tasks.reserved_bytes = reserve_bytes;
        self.channel_tasks.source_frames = reserve_frames;
        self.channel_tasks.job = Some(Job {
            key,
            request,
            deadline,
            directory,
            instance,
            broker,
            cleanup,
            input_type,
            output_type,
            max_input_bytes,
            max_output_bytes,
            uploaded: vec![],
            uploaded_bytes: 0,
            run_admitted: false,
            worker: None,
            worker_started: false,
            worker_joined: false,
            report: None,
            error: String::new(),
            close_requested: false,
            disconnected: false,
            input_sha256: vec![],
            progress: Arc::new(Mutex::new(Progress::default())),
            last_progress: ProgressView::default(),
            last_snapshot: None,
            snapshot_pending: true,
        });
        self.channel_status(&key)
    }
    pub fn append_channel(&mut self, key: &[u8], frame: SourceFrame) -> Result<State> {
        self.channel_tasks.touch(false)?;
        self.product_gate().check()?;
        self.channel_reclaim()?;
        let job = self.channel_tasks.checked(key)?;
        if job.run_admitted
            || job.close_requested
            || Instant::now() >= job.deadline
            || frame.sequence != job.uploaded.len() as u64 + 1
            || frame.bytes.is_empty()
            || frame.bytes.len() > job.request.budget.max_frame_bytes as usize
            || frame.cursor.len() > morrow_core::channel::MAX_CURSOR_BYTES
            || job.request.kind == Kind::ByteStream && !frame.cursor.is_empty()
            || job.uploaded.len() >= job.request.frame_count as usize
        {
            return Err("invalid or repeated local source append".into());
        }
        let bytes = job
            .uploaded_bytes
            .checked_add(frame.bytes.len() as u64)
            .ok_or("source byte overflow")?;
        if bytes > job.request.total_bytes {
            return Err("source bytes exceed declared preparation".into());
        }
        job.uploaded_bytes = bytes;
        job.uploaded.push(frame);
        self.channel_status(key)
    }
    pub fn run_channel(&mut self, key: &[u8], input: Vec<u8>) -> Result<State> {
        self.channel_tasks.touch(false)?;
        self.product_gate().check()?;
        self.channel_reclaim()?;
        let job = self.channel_tasks.checked(key)?;
        if job.run_admitted
            || job.close_requested
            || !job.ready()
            || input.len() > job.max_input_bytes as usize
        {
            return Err("channel run is invalid or already admitted; do not replay".into());
        }
        if job.input_type == DIRECTORY_INPUT_TYPE {
            let parsed = morrow_core::channel::Directory::decode(&input)?;
            if parsed.encode()? != job.directory {
                return Err("channel input directory belongs to another grant".into());
            }
        }
        let transform = Transform {
            handler: job.request.handler.clone(),
            input_type: job.input_type.clone(),
            output_type: job.output_type.clone(),
            input: input.clone(),
        };
        job.instance
            .package()
            .package()
            .channel_handler(&transform)?;
        let task_id = job
            .key
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        let invocation = Invocation::new_transform(&task_id, transform)?;
        // Take the single original owner before starting any native source.
        // No fallback state is opened if this handoff is busy or denied.
        let owner = self.state.take_channel_owner()?;
        let start = owner.start;
        // Irreversible admission flag precedes both thread starts. Lost replies never resubmit it.
        job.run_admitted = true;
        job.input_sha256 = Sha256::digest(&input).to_vec();
        let frames = job.uploaded.clone();
        let progress = job.progress.clone();
        let duplex = job.request.duplex;
        let cap = job.request.budget.max_bytes.min(MAX_JOB_BYTES);
        let spawned = job.broker.spawn(move |producer| {
            if !duplex {
                for frame in frames {
                    if producer.push(frame.bytes.clone(), frame.cursor).is_err() {
                        return;
                    }
                    let mut progress = progress.lock().unwrap_or_else(|e| e.into_inner());
                    progress.source_frames += 1;
                    progress.source_bytes += frame.bytes.len() as u64;
                }
                let _ = producer.finish();
            } else {
                // Production duplex grants are sink-only: no extra producer thread
                // or half-close interpretation of the public terminal Closed status.
                while let Ok((sequence, bytes)) = producer.wait_sent() {
                    let mut progress = progress.lock().unwrap_or_else(|e| e.into_inner());
                    if progress.sent.len() >= MAX_SOURCE_FRAMES as usize
                        || progress.observed_bytes.saturating_add(bytes.len() as u64) > cap
                    {
                        break;
                    }
                    progress.observed_bytes += bytes.len() as u64;
                    progress.observed_hash.update(&bytes);
                    progress.sent.push((sequence, bytes));
                }
                // An explicit guest Close can end a sink without claiming source EOF.
                let _ = producer.finish();
            }
        });
        if let Err(error) = spawned {
            self.state.restore_channel_owner(owner, false)?;
            job.error = short_error(error);
            job.stop();
            return self.channel_status(key);
        }
        let broker = job.broker.clone();
        let instance = job.instance.clone();
        let max_output_bytes = job.max_output_bytes;
        let handoff = Arc::new(Mutex::new(Some(owner)));
        let child_handoff = handoff.clone();
        let worker = thread::Builder::new()
            .name("morrow-channel-executor".into())
            .spawn(move || {
                let mut owner = child_handoff
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .take()
                    .expect("original channel owner handoff");
                let ran = catch_unwind(AssertUnwindSafe(|| {
                    let manager = owner.manager.as_ref().expect("original managed owner");
                    broker.run_invocation(manager, &mut owner.host, &instance, &invocation, || {
                        now(start)
                    })
                }));
                let (report, error) = match ran {
                    Ok(report)
                        if report.output.as_ref().is_none_or(|output| {
                            output.bytes.len() <= max_output_bytes as usize
                        }) =>
                    {
                        (Some(report), String::new())
                    }
                    Ok(_) => (
                        None,
                        "channel output exceeds the private result bound".into(),
                    ),
                    Err(_) => {
                        instance.request_stop();
                        (
                            None,
                            "channel executor panicked; business outcome Unknown".into(),
                        )
                    }
                };
                let maintenance = morrow_plugin_runtime::io_jobs::HostOwner::finish_io(&mut owner);
                Exit {
                    owner,
                    report,
                    error: if maintenance.is_err() {
                        "original storage maintenance failed; outcome Unknown".into()
                    } else {
                        error
                    },
                    repair: maintenance.is_err(),
                }
            });
        match worker {
            Ok(worker) => {
                job.worker_started = true;
                job.worker = Some(worker);
            }
            Err(error) => {
                let owner = handoff
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .take()
                    .ok_or(AccessError::OwnerUnavailable)?;
                self.state.restore_channel_owner(owner, false)?;
                job.error = short_error(error);
                job.stop();
            }
        }
        self.channel_status(key)
    }
    pub fn channel_status(&mut self, key: &[u8]) -> Result<State> {
        self.channel_tasks.checked(key)?;
        self.channel_tasks.touch(true)?;
        let gate_closed = self.product_gate().closed();
        if gate_closed {
            self.channel_tasks.request_stop();
        }
        self.channel_reclaim()?;
        let job = self.channel_tasks.checked(key)?;
        job.observe();
        Ok(job.state(gate_closed))
    }
    pub fn close_channel(&mut self, key: &[u8]) -> Result<State> {
        self.channel_tasks.checked(key)?;
        self.channel_tasks.touch(true)?;
        self.channel_tasks.checked(key)?.stop();
        self.channel_status(key)
    }
    pub fn read_channel_sent(
        &mut self,
        key: &[u8],
        sequence: u64,
        offset: u32,
        limit: u32,
    ) -> Result<Sent> {
        self.channel_tasks.checked(key)?;
        if limit == 0 || limit > 32 * 1024 {
            return Err("channel history chunk limit".into());
        }
        self.channel_tasks.touch(false)?;
        self.product_gate().check()?;
        self.channel_reclaim()?;
        let job = self.channel_tasks.checked(key)?;
        if job.close_requested || Instant::now() >= job.deadline || sequence == 0 {
            return Err("original channel read gate closed".into());
        }
        let progress = job.progress.try_lock().map_err(|_| AccessError::Busy)?;
        if progress
            .sent
            .iter()
            .any(|(observed, bytes)| *observed == sequence && offset as usize >= bytes.len())
        {
            return Err("channel history chunk offset".into());
        }
        Ok(progress
            .sent
            .iter()
            .find(|(observed, _)| *observed == sequence)
            .map_or(
                Sent {
                    present: false,
                    sequence,
                    bytes: vec![],
                    offset,
                    total_bytes: 0,
                    bytes_sha256: vec![],
                },
                |(sequence, bytes)| Sent {
                    present: true,
                    sequence: *sequence,
                    bytes: bytes
                        .get(offset as usize..)
                        .unwrap_or(&[])
                        .iter()
                        .take(limit as usize)
                        .copied()
                        .collect(),
                    offset,
                    total_bytes: bytes.len() as u32,
                    bytes_sha256: Sha256::digest(bytes).to_vec(),
                },
            ))
    }
    pub(crate) fn finish_channels(&mut self) -> Result<()> {
        self.channel_tasks.request_stop();
        self.channel_reclaim()?;
        if let Some(job) = &self.channel_tasks.job {
            if !job.disconnected || job.worker.is_some() {
                return Err(AccessError::Busy.into());
            }
        }
        Ok(())
    }
}
impl Drop for ChannelTasks {
    fn drop(&mut self) {
        self.request_stop();
    }
}
#[cfg(test)]
#[path = "channel_tasks_tests.rs"]
mod tests;
