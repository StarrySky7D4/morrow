//! Pure UI execution borrowing the one authoritative runtime in a native host process.
//! Calls are synchronous and fuel bounded: do not invoke them on a Flutter rendering thread.
//! No Store is created or owned here; document replacement is never a content commit.
use crate::{
    Cancellation, Fault,
    package::{PreparedPackage, TaskReport},
};
use morrow_core::{
    dispatch::{Connection, ConnectionBinding, HostRuntime},
    lifecycle::InstancePhase,
    task::{Invocation, PluginFailure, Transform},
    ui::{self, Document},
};
use crate::monotonic::Instant;
use std::time::Duration;
const DOCUMENT: &str = "morrow.ui.document.v1";
#[derive(Debug)]
pub enum Error {
    Core(morrow_core::Error),
    Binding,
    Unavailable,
    Closed,
    AlreadyOpened,
    InvalidTimeout,
}
impl From<morrow_core::Error> for Error {
    fn from(e: morrow_core::Error) -> Self {
        Self::Core(e)
    }
}
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "inline UI: {self:?}")
    }
}
impl std::error::Error for Error {}
#[derive(Debug)]
pub enum Failure {
    Execution(Fault),
    Plugin(PluginFailure),
    InvalidDocument,
    Revoked,
}
#[derive(Debug)]
pub struct Reply {
    pub view: String,
    pub generation: u64,
    pub revision: u64,
    pub serial: u64,
    pub document: Option<Vec<u8>>,
    pub failure: Option<Failure>,
}
pub struct InlineUi {
    session: ui::Session,
    digest: [u8; 32],
    binding: ConnectionBinding,
    view: String,
    generation: u64,
    serial: u64,
    next: u64,
    closed: bool,
    timeout: Duration,
}
impl InlineUi {
    pub fn new(
        package: &PreparedPackage,
        host: &HostRuntime,
        connection: &Connection,
        view: &str,
        generation: u64,
        timeout: Duration,
    ) -> Result<Self, Error> {
        if timeout.is_zero() || timeout > Duration::from_secs(3600) {
            return Err(Error::InvalidTimeout);
        }
        let session = ui::Session::new(view, generation)?;
        if package.package().manifest().guest_abi_version != 2 {
            return Err(Error::Binding);
        }
        for (handler, input_type, size) in [
            ("ui.form", "text.utf8", 32),
            ("ui.edit", "morrow.ui.event.v1", ui::MAX_BYTES),
        ] {
            let registration = package
                .package()
                .transform_handler(&transform(handler, input_type, vec![0; size]))
                .map_err(|_| Error::Binding)?;
            if registration.max_output_bytes as usize != ui::MAX_BYTES {
                return Err(Error::Binding);
            }
        }
        let result = Self {
            session,
            digest: package.package().digest(),
            binding: connection.binding(),
            view: view.into(),
            generation,
            serial: 0,
            next: 1,
            closed: false,
            timeout,
        };
        result.available(package, host, connection)?;
        Ok(result)
    }
    pub fn view(&self) -> &str {
        &self.view
    }
    pub fn generation(&self) -> u64 {
        self.generation
    }
    pub fn revision(&self) -> u64 {
        self.session.revision()
    }
    /// Last admitted event, including failed executions; never replay it automatically.
    pub fn serial(&self) -> u64 {
        self.serial
    }
    pub fn package_digest(&self) -> [u8; 32] {
        self.digest
    }
    pub fn close(&mut self) {
        self.closed = true;
        self.session.close();
    }
    fn available(
        &self,
        package: &PreparedPackage,
        host: &HostRuntime,
        connection: &Connection,
    ) -> Result<(), Error> {
        if self.closed || self.next == u64::MAX {
            return Err(Error::Closed);
        }
        if self.digest != package.package().digest()
            || connection.package_digest() != Some(self.digest)
            || connection.binding() != self.binding
        {
            return Err(Error::Binding);
        }
        if host.connection_phase(connection) != Ok(InstancePhase::Ready) {
            return Err(Error::Unavailable);
        }
        Ok(())
    }
    pub fn open(
        &mut self,
        package: &PreparedPackage,
        host: &mut HostRuntime,
        connection: &Connection,
        seed: &str,
    ) -> Result<Reply, Error> {
        self.available(package, host, connection)?;
        if self.revision() != 0 {
            return Err(Error::AlreadyOpened);
        }
        if seed.len() > 32 {
            return Err(Error::Core(morrow_core::Error::Limit));
        }
        let input = self.invocation("ui.form", "text.utf8", seed.as_bytes().to_vec())?;
        Ok(self.execute(package, host, connection, input))
    }
    /// Rejected bytes consume no serial. After admission, guest failure retains the serial.
    pub fn event(
        &mut self,
        package: &PreparedPackage,
        host: &mut HostRuntime,
        connection: &Connection,
        bytes: &[u8],
    ) -> Result<Reply, Error> {
        self.available(package, host, connection)?;
        if bytes.len() > ui::MAX_BYTES {
            return Err(Error::Core(morrow_core::Error::Limit));
        }
        let input = self.invocation("ui.edit", "morrow.ui.event.v1", bytes.to_vec())?;
        let event = self.session.accept(bytes)?;
        self.serial = event.serial;
        Ok(self.execute(package, host, connection, input))
    }
    fn invocation(
        &self,
        handler: &str,
        input_type: &str,
        input: Vec<u8>,
    ) -> Result<Invocation, Error> {
        Ok(Invocation::new_transform(
            &format!("inline-ui-{}", self.next),
            transform(handler, input_type, input),
        )?)
    }
    fn execute(
        &mut self,
        package: &PreparedPackage,
        host: &mut HostRuntime,
        connection: &Connection,
        input: Invocation,
    ) -> Reply {
        self.next += 1;
        let base = self.revision();
        let mut reply = Reply {
            view: self.view.clone(),
            generation: self.generation,
            revision: base,
            serial: self.serial,
            document: None,
            failure: None,
        };
        let Some(deadline) = Instant::now().checked_add(self.timeout) else {
            reply.failure = Some(Failure::Execution(Fault::Limits));
            return reply;
        };
        let report = package.run_task(
            host,
            connection,
            &input,
            || 0,
            Cancellation::until(deadline),
        );
        // Revocation can arrive from another native control thread while pure code executes.
        if host.connection_phase(connection) != Ok(InstancePhase::Ready) {
            self.close();
            reply.failure = Some(Failure::Revoked);
            return reply;
        }
        match decode(report).and_then(|document| {
            let bytes = document.encode().map_err(|_| Failure::InvalidDocument)?;
            let revision = self
                .session
                .replace(base, document)
                .map_err(|_| Failure::InvalidDocument)?;
            Ok((revision, bytes))
        }) {
            Ok((revision, bytes)) => {
                reply.revision = revision;
                reply.document = Some(bytes);
            }
            Err(failure) => reply.failure = Some(failure),
        }
        reply
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
