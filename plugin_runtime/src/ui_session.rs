//! Online, single-flight pure UI sessions. A document update is NEVER a content commit.
//! The trusted caller selects a fresh generation and registers/renders the view separately.
use crate::{
    Fault,
    package::{PreparedPackage, TaskReport},
    worker::{TaskHandle, Worker, WorkerError},
};
use morrow_core::{
    dispatch::HostRuntime,
    task::{Invocation, PluginFailure, Transform},
    ui::{self, Document},
};
use std::time::Duration;
const DOCUMENT: &str = "morrow.ui.document.v1";

#[derive(Debug)]
pub enum Error {
    Core(morrow_core::Error),
    Worker(WorkerError),
    Busy,
    Closed,
    AlreadyOpened,
    Binding,
}
impl From<morrow_core::Error> for Error {
    fn from(e: morrow_core::Error) -> Self {
        Self::Core(e)
    }
}
impl From<WorkerError> for Error {
    fn from(e: WorkerError) -> Self {
        Self::Worker(e)
    }
}
#[derive(Debug)]
pub enum Failure {
    Execution(Fault),
    Plugin(PluginFailure),
    Worker(WorkerError),
    InvalidDocument,
}
/// Host ticket, not a permission token. Serial zero identifies the initial form task.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Ticket {
    pub request: u64,
    pub serial: u64,
    pub base_revision: u64,
}
#[derive(Debug)]
pub enum Update {
    /// Validated UI-only snapshot; there is intentionally no saved/committed flag.
    Document {
        ticket: Ticket,
        revision: u64,
        document: Document,
    },
    Failed {
        ticket: Ticket,
        failure: Failure,
    },
}
enum Pending {
    Task(Ticket, TaskHandle<TaskReport>),
    Failed(Ticket, WorkerError),
}
pub struct UiSession {
    worker: Worker,
    session: ui::Session,
    digest: [u8; 32],
    view: String,
    generation: u64,
    pending: Option<Pending>,
    next: u64,
    serial: u64,
    closed: bool,
    timeout: Duration,
}
impl UiSession {
    /// Owns a dedicated worker and its actual package connection; arbitrary guest identities
    /// cannot be substituted. This adapter supports the fixed experimental form/edit handlers.
    pub fn new(
        package: PreparedPackage,
        mut host: HostRuntime,
        view: &str,
        generation: u64,
        timeout: Duration,
    ) -> Result<Self, Error> {
        if timeout.is_zero() || timeout > Duration::from_secs(3600) {
            return Err(Error::Worker(WorkerError::InvalidOptions));
        }
        let session = ui::Session::new(view, generation)?;
        for (handler, input_type, size) in [
            ("ui.form", "text.utf8", 32),
            ("ui.edit", "morrow.ui.event.v1", ui::MAX_BYTES),
        ] {
            let t = transform(handler, input_type, vec![0; size]);
            let registration = package
                .package()
                .transform_handler(&t)
                .map_err(|_| Error::Binding)?;
            if registration.max_output_bytes as usize != ui::MAX_BYTES {
                return Err(Error::Binding);
            }
        }
        if package.package().manifest().guest_abi_version != 2 {
            return Err(Error::Binding);
        }
        let digest = package.package().digest();
        let connection = package.connect(&mut host)?;
        // UI pure transforms reject every core exchange before dispatch; this clock is unreachable.
        let worker = Worker::spawn(package, host, connection, || 0, 1)?;
        Ok(Self {
            worker,
            session,
            digest,
            view: view.into(),
            generation,
            pending: None,
            next: 1,
            serial: 0,
            closed: false,
            timeout,
        })
    }
    pub fn package_digest(&self) -> [u8; 32] {
        self.digest
    }
    pub fn view(&self) -> &str {
        &self.view
    }
    pub fn generation(&self) -> u64 {
        self.generation
    }
    /// Execution retirement is separate from consuming its queued result.
    pub fn execution_pending(&self) -> bool {
        self.worker.pending() != 0
    }
    pub fn revision(&self) -> u64 {
        self.session.revision()
    }
    /// Last admitted event, including failures and cancellation. Never replay it automatically.
    pub fn serial(&self) -> u64 {
        self.serial
    }
    fn available(&self) -> Result<(), Error> {
        if self.closed {
            return Err(Error::Closed);
        }
        if self.pending.is_some() || self.worker.pending() != 0 {
            return Err(Error::Busy);
        }
        if self.next == u64::MAX {
            return Err(Error::Closed);
        }
        Ok(())
    }
    pub fn open_form(&mut self, seed: &str) -> Result<Ticket, Error> {
        self.available()?;
        if self.revision() != 0 {
            return Err(Error::AlreadyOpened);
        }
        if seed.len() > 32 {
            return Err(Error::Core(morrow_core::Error::Limit));
        }
        let invocation = self.invocation("ui.form", "text.utf8", seed.as_bytes().to_vec())?;
        Ok(self.enqueue(invocation, 0))
    }
    /// Busy/rejected events are not admitted. After admission this always returns a ticket;
    /// enqueue/guest failures are polled as Failed and the event serial remains consumed.
    pub fn event(&mut self, bytes: &[u8]) -> Result<Ticket, Error> {
        self.available()?;
        if bytes.len() > ui::MAX_BYTES {
            return Err(Error::Core(morrow_core::Error::Limit));
        }
        let invocation = self.invocation("ui.edit", "morrow.ui.event.v1", bytes.to_vec())?;
        let event = self.session.accept(bytes)?;
        self.serial = event.serial;
        Ok(self.enqueue(invocation, event.serial))
    }
    fn invocation(
        &self,
        handler: &str,
        input_type: &str,
        input: Vec<u8>,
    ) -> Result<Invocation, Error> {
        Ok(Invocation::new_transform(
            &format!("ui-{}", self.next),
            transform(handler, input_type, input),
        )?)
    }
    fn enqueue(&mut self, invocation: Invocation, serial: u64) -> Ticket {
        let ticket = Ticket {
            request: self.next,
            serial,
            base_revision: self.revision(),
        };
        self.next += 1;
        self.pending = Some(match self.worker.submit_task(invocation, self.timeout) {
            Ok(task) => Pending::Task(ticket, task),
            Err(error) => Pending::Failed(ticket, error),
        });
        ticket
    }
    pub fn poll(&mut self) -> Option<Update> {
        if self.closed {
            return None;
        }
        let (ticket, result) = match self.pending.as_mut()? {
            Pending::Failed(ticket, e) => (*ticket, Err(Failure::Worker(*e))),
            Pending::Task(ticket, task) => match task.try_result() {
                Ok(None) => return None,
                Ok(Some(report)) => (*ticket, decode(report)),
                Err(e) => (*ticket, Err(Failure::Worker(e))),
            },
        };
        self.pending = None;
        Some(match result {
            Ok(document) => match self.session.replace(ticket.base_revision, document.clone()) {
                Ok(revision) => Update::Document {
                    ticket,
                    revision,
                    document,
                },
                Err(_) => Update::Failed {
                    ticket,
                    failure: Failure::InvalidDocument,
                },
            },
            Err(failure) => Update::Failed { ticket, failure },
        })
    }
    /// Discards the result even if it already completed. Worker execution may still be running;
    /// further admission stays Busy until its bounded task retires. Returns the consumed ticket.
    pub fn cancel_pending(&mut self) -> Option<Ticket> {
        self.pending.take().map(|p| match p {
            Pending::Task(t, _) | Pending::Failed(t, _) => t,
        })
    }
    pub fn close(&mut self) {
        self.closed = true;
        self.session.close();
        self.cancel_pending();
        self.worker.stop();
    }
    /// Reclaims the core only after actual worker termination; never blocks the rendering thread.
    pub fn try_finish(&mut self) -> Result<Option<HostRuntime>, Error> {
        Ok(self.worker.try_finish()?)
    }
}
fn transform(handler: &str, input_type: &str, input: Vec<u8>) -> Transform {
    Transform {
        handler: handler.into(),
        input_type: input_type.into(),
        output_type: DOCUMENT.into(),
        input,
    }
}
fn decode(report: TaskReport) -> Result<Document, Failure> {
    if let Err(fault) = report.execution.outcome {
        return Err(Failure::Execution(fault));
    }
    if report.execution.outcome != Ok(0)
        || report.execution.host_calls != 0
        || report.response.is_some()
    {
        return Err(Failure::InvalidDocument);
    }
    if let Some(failure) = report.failure {
        return Err(Failure::Plugin(failure));
    }
    let output = report.output.ok_or(Failure::InvalidDocument)?;
    if output.type_id != DOCUMENT {
        return Err(Failure::InvalidDocument);
    }
    Document::decode(&output.bytes).map_err(|_| Failure::InvalidDocument)
}
