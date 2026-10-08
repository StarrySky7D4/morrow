use std::sync::{Arc, Mutex};

use morrow_agent_session_exec_v1_r2::{
    Action, Error, Outcome, Reply, Request, Result,
    authority::{Admission, SessionExecHost},
    safe_exec::ToolObservation,
};
use morrow_core::dispatch::{Connection, HostRuntime};

/// Original native authority. Construct only from a trusted owner's live objects.
/// This API does not create another Store, owner, connection, or admission.
pub struct NativeR2State {
    pub runtime: HostRuntime,
    pub host: SessionExecHost,
    pub proposer_connection: Connection,
    pub executor_connection: Arc<Connection>,
    pub proposer: Admission,
    pub executor: Admission,
    pub session_id: String,
    pub clock: Arc<dyn Fn() -> u64 + Send + Sync>,
    pub native_executions: Arc<crate::NativeExecutionRegistry>,
}

pub type SharedNativeR2State = Arc<Mutex<NativeR2State>>;

impl NativeR2State {
    pub fn with_runtime<T>(
        shared: &SharedNativeR2State,
        f: impl FnOnce(&mut HostRuntime, &SessionExecHost) -> Result<T>,
    ) -> Result<T> {
        let mut state = shared.lock().map_err(|_| Error::Storage)?;
        let Self { runtime, host, .. } = &mut *state;
        f(runtime, host)
    }

    pub fn with_state<T>(
        shared: &SharedNativeR2State,
        f: impl FnOnce(&mut Self) -> Result<T>,
    ) -> Result<T> {
        let mut state = shared.lock().map_err(|_| Error::Storage)?;
        f(&mut state)
    }
    pub fn dispatch(&mut self, request: &Request, executor: bool) -> Result<Outcome> {
        let (connection, admission) = if executor {
            (self.executor_connection.as_ref(), &self.executor)
        } else {
            (&self.proposer_connection, &self.proposer)
        };
        let raw = self.host.dispatch(
            &mut self.runtime,
            connection,
            admission,
            request.raw(),
            || (self.clock)(),
        )?;
        Reply::decode_for(request, &raw).map(|reply| reply.outcome)
    }

    /// A session-read query on the original connection, never an execution grant.
    /// Start and process controls must additionally call validate_started_tool.
    pub fn inspect_live(&mut self, operation: &str) -> Result<ToolObservation> {
        let request = Request::new_for_generation(
            format!("native-delivery-{operation}"),
            self.host.generation(&self.runtime)?,
            Action::Inspect {
                operation_id: operation.into(),
            },
        )?;
        match self.dispatch(&request, true)? {
            Outcome::Tool(_) => self.host.inspect_tool_record(&self.runtime, operation),
            Outcome::Rejected(error) => Err(error),
            _ => Err(Error::Denied),
        }
    }
}
