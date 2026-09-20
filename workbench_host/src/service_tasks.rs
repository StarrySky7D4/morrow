//! Trusted application service admission. Persistent records select the exact
//! listener; they never restore a live grant or a previous submission.
use super::{AccessError, Executor, ExitStatus, Snapshot, Task, TaskKey};
use crate::{Result, Workbench, WorkbenchState, now};
use morrow_core::{
    io::Request,
    plugin_package::io::{IoCapability, MAX_SERVICE_RUN_DURATION_MS},
    service_authority::proto::record::Kind,
};
use morrow_network_node::{
    Error as NetworkError, Limits as NetworkLimits,
    managed_service::{ManagedNode, RouterFactory, ServiceHost},
};
use morrow_plugin_runtime::{
    io_binding::ServiceRunBudget,
    io_jobs::{
        BrokerRouter, IoWorker, JobError, JobLimits, OwnerCommandHandle, RouteContext, RouterFault,
        WorkerExit,
    },
    service_authority::{ConfiguredService, ResolvedService},
    service_io::ListenerGrant,
};
use std::{
    collections::BTreeSet,
    net::SocketAddr,
    panic::{AssertUnwindSafe, catch_unwind},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    thread::JoinHandle,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use zeroize::Zeroizing;

pub struct ServiceStart {
    pub submission: [u8; 32],
    pub config_id: String,
    pub config_digest: [u8; 32],
    pub config_revision: u64,
    pub publication: [u8; 32],
    pub publication_revision: u64,
    pub package_id: String,
    pub package_digest: [u8; 32],
    pub registry_revision: u64,
    pub lifetime: Duration,
    pub budget: ServiceRunBudget,
    pub limits: JobLimits,
    pub network_limits: NetworkLimits,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ServicePhase {
    Starting,
    Running,
    Stopping,
    Exited,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ServiceSnapshot {
    pub task: Snapshot,
    pub submission: [u8; 32],
    pub phase: ServicePhase,
    pub address: Option<SocketAddr>,
    pub bind: Option<std::result::Result<(), NetworkError>>,
    pub listener: Option<std::result::Result<(), NetworkError>>,
    pub supervision: Option<std::result::Result<(), NetworkError>>,
}
#[derive(Clone, Copy)]
pub(super) struct Progress {
    submission: [u8; 32],
    pub(super) phase: ServicePhase,
    address: Option<SocketAddr>,
    bind: Option<std::result::Result<(), NetworkError>>,
    listener: Option<std::result::Result<(), NetworkError>>,
    supervision: Option<std::result::Result<(), NetworkError>>,
}
impl Default for Progress {
    fn default() -> Self {
        Self {
            submission: [0; 32],
            phase: ServicePhase::Starting,
            address: None,
            bind: None,
            listener: None,
            supervision: None,
        }
    }
}
struct Control {
    stop: AtomicBool,
    progress: Mutex<Progress>,
}
impl Control {
    fn progress(&self) -> std::sync::MutexGuard<'_, Progress> {
        self.progress.lock().unwrap_or_else(|e| e.into_inner())
    }
}
pub(super) struct ServiceExecution {
    host: ServiceHost<WorkbenchState>,
    listener: ListenerGrant,
    control: Arc<Control>,
    // The supervisor owns the runtime until both listener and worker exit. A
    // dropped application detaches this thread, never its cleanup resources.
    join: Option<JoinHandle<std::result::Result<WorkerExit<WorkbenchState>, JobError>>>,
}
impl ServiceExecution {
    fn launch(
        host: ServiceHost<WorkbenchState>,
        configured: ConfiguredService,
        runtime: tokio::runtime::Runtime,
        limits: NetworkLimits,
        submission: [u8; 32],
    ) -> Self {
        let listener = configured.listener().clone();
        let control = Arc::new(Control {
            stop: AtomicBool::new(false),
            progress: Mutex::new(Progress {
                submission,
                ..Default::default()
            }),
        });
        let worker_host = host.clone();
        let worker_control = control.clone();
        let worker_listener = listener.clone();
        let joined = std::thread::Builder::new()
            .name("morrow-service-supervisor".into())
            .spawn(move || {
                supervise(
                    runtime,
                    worker_host,
                    configured,
                    worker_listener,
                    worker_control,
                    limits,
                )
            });
        let join = match joined {
            Ok(join) => Some(join),
            Err(_) => {
                // No listener future was polled. The retained original host
                // remains available for nonblocking worker reclamation.
                listener.revoke();
                let _ = host.request_stop();
                let mut progress = control.progress();
                progress.phase = ServicePhase::Stopping;
                progress.bind = Some(Err(NetworkError::Transport));
                progress.listener = Some(Ok(()));
                progress.supervision = Some(Err(NetworkError::Transport));
                None
            }
        };
        Self {
            host,
            listener,
            control,
            join,
        }
    }
    pub(super) fn progress(&self) -> Progress {
        *self.control.progress()
    }
    pub(super) fn stop(&self) {
        self.control.stop.store(true, Ordering::Release);
        self.listener.revoke();
        let _ = self.host.request_stop();
        let mut progress = self.control.progress();
        if progress.phase != ServicePhase::Exited {
            progress.phase = ServicePhase::Stopping;
        }
    }
    pub(super) fn try_reclaim(
        &mut self,
    ) -> std::result::Result<Option<WorkerExit<WorkbenchState>>, JobError> {
        if let Some(join) = &self.join {
            if !join.is_finished() {
                return Ok(None);
            }
            let result = self.join.take().ok_or(JobError::Consumed)?.join();
            return result.map_err(|_| JobError::Unavailable)?.map(Some);
        }
        // Only used when supervisor spawn failed before binding anything.
        self.host.try_reclaim().map_err(|_| JobError::Unavailable)
    }
    fn command(&self, input: Vec<u8>) -> Result<OwnerCommandHandle> {
        let mut input = Zeroizing::new(input);
        if self.control.stop.load(Ordering::Acquire)
            || self.progress().phase != ServicePhase::Running
        {
            return Err(AccessError::Busy.into());
        }
        Ok(self
            .host
            .submit_owner_command(std::mem::take(&mut *input), 128 * 1024)?)
    }
}
impl Drop for ServiceExecution {
    fn drop(&mut self) {
        self.stop();
    }
}

fn supervise(
    runtime: tokio::runtime::Runtime,
    host: ServiceHost<WorkbenchState>,
    configured: ConfiguredService,
    listener: ListenerGrant,
    control: Arc<Control>,
    limits: NetworkLimits,
) -> std::result::Result<WorkerExit<WorkbenchState>, JobError> {
    // Keep the node outside the unwind boundary so a panic while supervising
    // still leaves its actual join handle available for explicit cleanup.
    let mut node: Option<ManagedNode> = None;
    let operation = catch_unwind(AssertUnwindSafe(|| {
        runtime.block_on(async {
            if control.stop.load(Ordering::Acquire) {
                control.progress().bind = Some(Err(NetworkError::Cancelled));
                return;
            }
            match host.bind_configured(configured, None, limits).await {
                Ok(bound) => {
                    let address = bound.local_addr();
                    node = Some(bound);
                    let mut progress = control.progress();
                    progress.address = Some(address);
                    progress.bind = Some(Ok(()));
                    progress.phase = if control.stop.load(Ordering::Acquire) {
                        ServicePhase::Stopping
                    } else {
                        ServicePhase::Running
                    };
                }
                Err(error) => {
                    control.progress().bind = Some(Err(error));
                    return;
                }
            }
            while !control.stop.load(Ordering::Acquire)
                && node.as_ref().is_some_and(|node| !node.is_finished())
            {
                tokio::time::sleep(Duration::from_millis(2)).await;
            }
        })
    }));
    if operation.is_err() {
        let mut progress = control.progress();
        progress.bind.get_or_insert(Err(NetworkError::Transport));
    }
    control.progress().supervision = Some(operation.map_err(|_| NetworkError::Transport));
    control.stop.store(true, Ordering::Release);
    control.progress().phase = ServicePhase::Stopping;
    listener.revoke();
    let _ = host.request_stop();
    let listener_result = runtime.block_on(async {
        if let Some(node) = &mut node {
            node.request_stop();
            node.join().await
        } else {
            Ok(())
        }
    });
    control.progress().listener = Some(listener_result);
    // Even listener failure cannot release the worker-owned Storage early.
    let result = runtime
        .block_on(host.shutdown_owned())
        .map_err(|_| JobError::Unavailable);
    // Only StateSlot marks Exited after joining this supervisor. Returning a
    // worker result is not proof that runtime teardown and this thread ended.
    result
}

struct DenyOutbound;
impl BrokerRouter for DenyOutbound {
    fn route(
        &mut self,
        _: &mut RouteContext<'_>,
        _: u32,
        _: &Request,
    ) -> std::result::Result<Vec<u8>, RouterFault> {
        Err(RouterFault::Denied)
    }
}
fn utc() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .and_then(|time| time.as_millis().try_into().ok())
        .unwrap_or(0)
}
impl Workbench {
    /// Start one explicitly approved finite service. Returns before socket
    /// binding; poll the same task for its definitive binding result.
    pub fn start_service(&mut self, options: ServiceStart) -> Result<TaskKey> {
        self.state.try_reclaim()?;
        self.state.require_writable()?;
        if self.state.task.is_some() {
            return Err(AccessError::UnacknowledgedTask.into());
        }
        if options.submission == [0; 32]
            || self.state.service_submissions.contains(&options.submission)
            || self.state.service_submissions.len() >= 512
        {
            return Err("service submission is invalid, already used, or at capacity".into());
        }
        options.network_limits.validate()?;
        if options.lifetime.as_millis() == 0
            || options.lifetime.as_millis() > u128::from(MAX_SERVICE_RUN_DURATION_MS)
            || options.network_limits.timeout > Duration::from_secs(30)
            || options.network_limits.timeout > options.lifetime
        {
            return Err("invalid finite service limits".into());
        }
        // Build before moving the owner; runtime setup failure has no listener
        // or worker to clean up. Only the supervisor later drives its futures.
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()?;
        let mut key = [0; 32];
        getrandom::fill(&mut key)?;
        let key = TaskKey(key);
        let state = self.state.local_mut()?;
        let manager = state.manager.as_mut().ok_or("plugin manager unavailable")?;
        let selection = manager
            .selection(&options.package_id)
            .ok_or("plugin not selected")?;
        if manager.revision() != options.registry_revision
            || selection.digest != options.package_digest
            || !selection.enabled
        {
            return Err("plugin approval changed".into());
        }
        state.host.prepare_write()?;
        // Pin the authority writer before checking the caller's expected
        // revisions and address policy. A second Store must not replace those
        // records between our comparison and grant resolution.
        let resolved = ResolvedService::resolve(
            state.host.store_local_mut(),
            &options.config_id,
            &options.publication,
            utc,
        )?;
        let store = state.host.store_local();
        let config = store
            .load_service_config(&options.config_id)?
            .ok_or("service config missing")?;
        let approval = store
            .load_service_authority(&options.publication)?
            .ok_or("publication missing")?;
        if config.digest() != options.config_digest
            || config.value().revision != options.config_revision
            || config.value().package_sha256 != options.package_digest
            || approval.value().revision != options.publication_revision
        {
            return Err("service configuration or publication changed".into());
        }
        let Some(Kind::Publication(publication)) = approval.value().kind.as_ref() else {
            return Err("service publication required".into());
        };
        let address: SocketAddr = publication.listen_address.parse()?;
        if publication.tls_required || !address.ip().is_loopback() {
            return Err(
                "application service admission currently requires approved loopback HTTP".into(),
            );
        }
        let start = state.start;
        let time = now(start);
        let expires = time
            .checked_add(options.lifetime.as_millis().try_into()?)
            .ok_or("service expiry overflow")?;
        let instance = manager.connect(&options.package_id, &mut state.host)?;
        let prepared = catch_unwind(AssertUnwindSafe(|| -> Result<_> {
            let caps = BTreeSet::from([IoCapability::HttpListen, IoCapability::HttpPublish]);
            let binding = manager.bind_budgeted_service_run(
                &state.host,
                &instance,
                options.package_digest,
                options.registry_revision,
                &caps,
                expires,
                time,
                options.budget,
            )?;
            let configured = resolved.issue(manager, &state.host, &instance, &binding, time)?;
            Ok((binding, configured))
        }))
        .unwrap_or_else(|_| Err("service preparation panicked; no listener started".into()));
        let (binding, configured) = match prepared {
            Ok(prepared) => prepared,
            Err(error) => {
                if self.state.cleanup_instance(instance).is_err() {
                    self.state.task = Some(Task {
                        commands: Default::default(),
                        key,
                        worker: None,
                        handle: None,
                        stopping: false,
                        exit: Some(ExitStatus {
                            execution: Err(JobError::InvalidOptions),
                            disconnect: Err(JobError::Disconnect),
                            maintenance: Ok(()),
                        }),
                        service: Some(Progress {
                            submission: options.submission,
                            phase: ServicePhase::Exited,
                            bind: Some(Err(NetworkError::Denied)),
                            ..Default::default()
                        }),
                    });
                }
                return Err(error);
            }
        };
        self.state.service_submissions.insert(options.submission);
        let owner = self.state.owner.take().ok_or(AccessError::Busy)?;
        let worker = match IoWorker::spawn_managed_owner(
            owner,
            instance,
            binding,
            move || now(start),
            options.network_limits.max_concurrent,
            options.limits,
        ) {
            Ok(worker) => worker,
            Err(failure) => {
                self.state.owner = Some(failure.owner);
                let disconnect = if let Some(instance) = failure.instance {
                    self.state
                        .cleanup_instance(instance)
                        .map_err(|_| JobError::Disconnect)
                } else {
                    Ok(())
                };
                self.state.task = Some(Task {
                    commands: Default::default(),
                    key,
                    worker: None,
                    handle: None,
                    stopping: false,
                    exit: Some(ExitStatus {
                        execution: Err(failure.error),
                        disconnect,
                        maintenance: Ok(()),
                    }),
                    service: Some(Progress {
                        submission: options.submission,
                        phase: ServicePhase::Exited,
                        bind: Some(Err(NetworkError::Denied)),
                        ..Default::default()
                    }),
                });
                return Err("service worker admission failed; inspect task before retrying".into());
            }
        };
        let routers: RouterFactory = Arc::new(|| Box::new(DenyOutbound));
        let host = match ServiceHost::new_owned(worker, options.network_limits.timeout, routers) {
            Ok(host) => host,
            Err(failure) => {
                failure.worker.stop();
                self.state.task = Some(Task {
                    commands: Default::default(),
                    key,
                    worker: Some(Executor::Io(Box::new(failure.worker))),
                    handle: None,
                    stopping: true,
                    exit: None,
                    service: Some(Progress {
                        submission: options.submission,
                        phase: ServicePhase::Stopping,
                        bind: Some(Err(failure.error)),
                        ..Default::default()
                    }),
                });
                return Err(
                    "service adapter admission failed; inspect task before retrying".into(),
                );
            }
        };
        let execution = ServiceExecution::launch(
            host,
            configured,
            runtime,
            options.network_limits,
            options.submission,
        );
        self.state.task = Some(Task {
            commands: Default::default(),
            key,
            service: Some(execution.progress()),
            worker: Some(Executor::Service(execution)),
            handle: None,
            stopping: false,
            exit: None,
        });
        Ok(key)
    }
    pub fn service_status(&mut self, key: TaskKey) -> Result<ServiceSnapshot> {
        self.state.checked_task(key)?;
        let _ = self.state.try_reclaim();
        let progress = self
            .state
            .checked_task(key)?
            .service
            .ok_or(AccessError::StaleTask)?;
        Ok(ServiceSnapshot {
            task: self.state.snapshot(),
            submission: progress.submission,
            phase: progress.phase,
            address: progress.address,
            bind: progress.bind,
            listener: progress.listener,
            supervision: progress.supervision,
        })
    }
    /// Local host-only handle. Callers own successfully read secret-bearing
    /// frames and must wipe them; enqueue is never a successful business result.
    pub fn submit_service_command(
        &mut self,
        key: TaskKey,
        input: Vec<u8>,
    ) -> Result<OwnerCommandHandle> {
        let mut input = Zeroizing::new(input);
        let task = self.state.checked_task(key)?;
        let Some(Executor::Service(service)) = &task.worker else {
            return Err(AccessError::StaleTask.into());
        };
        service.command(std::mem::take(&mut *input))
    }
}

#[cfg(all(test, target_os = "windows"))]
#[path = "service_tasks_tests.rs"]
mod tests;
