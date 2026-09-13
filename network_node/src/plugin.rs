//! Explicit trusted route adapter for an installed immutable pure byte transformer.
//! Remote callers acquire no content grants and cannot select package or handler.
//! General inbound service manifests and guest-originated network jobs are not implemented here.
use crate::{Error, HttpResponse, Result, server::Handler};
use morrow_core::{
    dispatch::HostRuntime,
    plugin_package::catalog,
    store::{EventBudget, Store},
    task::{Invocation, Transform},
};
use morrow_plugin_runtime::{package::PreparedPackage, worker::Worker};
use std::{
    path::Path,
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};

pub struct PluginService {
    worker: Arc<Mutex<Worker>>,
    handler: String,
    max_input: usize,
    max_output: usize,
    sequence: Arc<AtomicU64>,
}
impl PluginService {
    /// The embedding host explicitly selects a package, handler and isolated store.
    /// No existing user content is opened or granted by this helper.
    pub fn open(package: &Path, new_directory: &Path, handler: &str) -> Result<Self> {
        // Validate before creating anything; the data directory must be entirely new.
        // Parent path is trusted; this does not resist same-account malicious path replacement.
        let p = catalog::read_file(package).map_err(|_| Error::Invalid)?;
        if !p.capabilities().is_empty() || !p.manifest().dependencies.is_empty() {
            return Err(Error::Denied);
        }
        let registration = p
            .manifest()
            .transform_handlers
            .iter()
            .find(|v| v.handler == handler && v.input_type == "bytes" && v.output_type == "bytes")
            .ok_or(Error::Denied)?;
        let (max_input, max_output) = (
            registration.max_input_bytes as usize,
            registration.max_output_bytes as usize,
        );
        let prepared = PreparedPackage::new(p, Default::default()).map_err(|_| Error::Invalid)?;
        std::fs::create_dir(new_directory).map_err(|_| Error::Denied)?;
        let store = Store::open(&new_directory.join("workbench.db"), EventBudget::default())
            .map_err(|_| Error::Transport)?;
        let mut host = HostRuntime::new(store).map_err(|_| Error::Transport)?;
        let connection = prepared
            .connect_approved(&mut host, &Default::default())
            .map_err(|_| Error::Denied)?;
        let worker =
            Worker::spawn(prepared, host, connection, || 0, 16).map_err(|_| Error::Transport)?;
        Ok(Self {
            worker: Arc::new(Mutex::new(worker)),
            handler: handler.into(),
            max_input,
            max_output,
            sequence: Arc::new(AtomicU64::new(1)),
        })
    }
    pub fn route_handler(&self) -> Handler {
        let worker = self.worker.clone();
        let handler = self.handler.clone();
        let sequence = self.sequence.clone();
        let (max_input, max_output) = (self.max_input, self.max_output);
        Arc::new(move |request, cancel| {
            let worker = worker.clone();
            let handler = handler.clone();
            let sequence = sequence.clone();
            Box::pin(async move {
                if cancel.is_cancelled() {
                    return Err(Error::Cancelled);
                }
                if request.body.len() > max_input {
                    return Err(Error::Limit);
                }
                let serial = sequence
                    .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |v| v.checked_add(1))
                    .map_err(|_| Error::Limit)?;
                let task = Invocation::new_transform(
                    &format!("api-{serial}"),
                    Transform {
                        handler,
                        input_type: "bytes".into(),
                        output_type: "bytes".into(),
                        input: request.body,
                    },
                )
                .map_err(|_| Error::Invalid)?;
                let mut handle = worker
                    .lock()
                    .map_err(|_| Error::Closed)?
                    .submit_task(task, Duration::from_secs(30))
                    .map_err(|_| Error::Limit)?;
                let result = loop {
                    if cancel.is_cancelled() {
                        return Err(Error::Cancelled);
                    }
                    if let Some(result) = handle.try_result().map_err(|_| Error::Closed)? {
                        break result;
                    }
                    tokio::select! {
                        biased;
                        _ = cancel.cancelled() => return Err(Error::Cancelled),
                        _ = tokio::time::sleep(Duration::from_millis(2)) => {}
                    }
                };
                if cancel.is_cancelled() {
                    return Err(Error::Cancelled);
                }
                if result.execution.outcome != Ok(0)
                    || result.execution.host_calls != 0
                    || result.response.is_some()
                {
                    return Err(Error::Transport);
                }
                if result.failure.is_some() {
                    return Ok(HttpResponse {
                        status: 422,
                        headers: vec![],
                        body: b"Plugin rejected input".to_vec(),
                    });
                }
                let output = result.output.ok_or(Error::Transport)?;
                if output.type_id != "bytes" || output.bytes.len() > max_output {
                    return Err(Error::Limit);
                }
                Ok(HttpResponse {
                    status: 200,
                    headers: vec![("content-type".into(), "application/octet-stream".into())],
                    body: output.bytes,
                })
            })
        })
    }
    pub async fn shutdown(&self) -> Result<()> {
        self.worker.lock().map_err(|_| Error::Closed)?.stop();
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            if let Some(host) = self
                .worker
                .lock()
                .map_err(|_| Error::Closed)?
                .try_finish()
                .map_err(|_| Error::Closed)?
            {
                host.store_local()
                    .integrity_check()
                    .map_err(|_| Error::Transport)?;
                return Ok(());
            }
            if Instant::now() >= deadline {
                return Err(Error::Timeout);
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    }
}
impl Drop for PluginService {
    fn drop(&mut self) {
        if let Ok(worker) = self.worker.lock() {
            worker.stop();
        }
    }
}
