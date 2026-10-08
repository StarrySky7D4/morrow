//! Concrete native port. Messages go to the existing exclusive original owner.
//! No Store/runtime/Workbench reference is placed in a second shared owner.
use super::{AgentCommand, AgentError, AgentReply, worker::NativeCommandQueue};
use morrow_agent_session_exec_v1_r2::{Action, Error, Outcome, Reply, Request};
use morrow_agent_session_process_v1_host::native_session::NativeEndpointAuthority;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};
use zeroize::Zeroizing;

struct EndpointClient {
    queue: NativeCommandQueue,
    authority: NativeEndpointAuthority,
    serial: Mutex<()>,
    uncertain: AtomicBool,
}
impl EndpointClient {
    fn live(&self) -> bool {
        let now = self.queue.now_millis();
        self.queue.live_at(now) && self.authority.is_live_at(now)
    }
}
// Once enqueued, unwinding or missing delivery cannot unlock a durable mutation.
struct MutationDelivery<'a> {
    client: &'a EndpointClient,
    armed: bool,
}
impl Drop for MutationDelivery<'_> {
    fn drop(&mut self) {
        if self.armed {
            self.client.uncertain.store(true, Ordering::Release);
            self.client.authority.mark_unknown();
        }
    }
}
impl Drop for EndpointClient {
    fn drop(&mut self) {
        // Revocation only. The original Context retains the server grant and
        // its close/disconnect debt; this is not a join or cleanup receipt.
        let _ = self.authority.close();
    }
}
/// Send + Sync + 'static transport suitable for OriginalNativeEndpoint adapters.
#[derive(Clone)]
pub struct NativeSessionEndpoint {
    client: Arc<EndpointClient>,
}
impl NativeSessionEndpoint {
    fn new(
        queue: NativeCommandQueue,
        authority: NativeEndpointAuthority,
    ) -> Result<Self, AgentError> {
        let now = queue.now_millis();
        if !queue.live_at(now) || !authority.is_live_at(now) {
            return Err(AgentError::Unknown);
        }
        Ok(Self {
            client: Arc::new(EndpointClient {
                queue,
                authority,
                serial: Mutex::new(()),
                uncertain: AtomicBool::new(false),
            }),
        })
    }
    pub fn generation(&self) -> u64 {
        self.client.authority.generation()
    }
    pub fn revoke(&self) -> Result<(), AgentError> {
        self.client.authority.close().map_err(AgentError::Session)
    }
    /// A trusted outer transport lost an already produced reply. This only reduces
    /// the original lease rights; reads and a second handle cannot clear it.
    pub fn mark_delivery_unknown(&self) {
        self.client.uncertain.store(true, Ordering::Release);
        self.client.authority.mark_unknown();
    }
    /// Submit exactly once and receive exactly once. Never renew TTL or resend.
    pub fn exchange_once(&self, canonical: &[u8]) -> Result<Vec<u8>, AgentError> {
        if canonical.len()
            > morrow_agent_session_process_v1_host::native_session::MAX_NATIVE_FRAME_BYTES
        {
            return Err(AgentError::Limit);
        }
        let _serial = self.client.serial.lock().map_err(|_| AgentError::Unknown)?;
        let request = Request::decode(canonical).map_err(AgentError::Session)?;
        if request.generation() != self.generation() {
            return Err(AgentError::Session(Error::Correlation));
        }
        if !self.client.live() {
            return Err(AgentError::Session(Error::Denied));
        }
        let reading = matches!(request.action(), Action::List | Action::Snapshot { .. });
        if !reading && self.client.uncertain.swap(true, Ordering::AcqRel) {
            return Err(AgentError::Session(Error::CommitUnknown));
        }
        let handle = match self
            .client
            .queue
            .submit(AgentCommand::NativeSessionExchange {
                endpoint: self.client.authority.id(),
                canonical: Zeroizing::new(canonical.to_vec()),
            }) {
            Ok(handle) => handle,
            Err(error) => {
                if !reading && matches!(error, AgentError::Busy | AgentError::Limit) {
                    self.client.uncertain.store(false, Ordering::Release);
                }
                return Err(error);
            }
        };
        let mut delivery = MutationDelivery {
            client: &self.client,
            armed: !reading,
        };
        let response = match handle.wait() {
            Ok(response) => response,
            Err(error) => {
                // An explicit pre-effect authority/schema/CAS rejection is
                // different from missing delivery or an uncertain durable write.
                if !reading
                    && matches!(
                        error,
                        AgentError::Session(
                            Error::Denied
                                | Error::Invalid
                                | Error::Contract
                                | Error::Correlation
                                | Error::Conflict
                                | Error::NotFound
                                | Error::Limit
                        )
                    )
                {
                    self.client.uncertain.store(false, Ordering::Release);
                    delivery.armed = false;
                }
                return Err(
                    if !reading
                        && delivery.armed
                        && !matches!(
                            error,
                            AgentError::Session(Error::CommitUnknown | Error::Storage)
                        )
                    {
                        AgentError::Unknown
                    } else {
                        error
                    },
                );
            }
        };
        // Native admission revocation is independent of the main Control.
        // Recheck both at delivery, even if the server just delivered a reply.
        if !self.client.live() {
            return Err(if reading {
                AgentError::Session(Error::Denied)
            } else {
                AgentError::Unknown
            });
        }
        let bytes = match &response {
            AgentReply::NativeSession(bytes) => bytes,
            _ => return Err(AgentError::Unknown),
        };
        let decoded = Reply::decode_for(&request, bytes).map_err(|_| AgentError::Unknown)?;
        let mut output = bytes.to_vec();
        if !self.client.live() {
            output.fill(0);
            return Err(if reading {
                AgentError::Session(Error::Denied)
            } else {
                AgentError::Unknown
            });
        }
        if !reading
            && !matches!(
                decoded.outcome,
                Outcome::Rejected(Error::CommitUnknown | Error::Storage)
            )
        {
            self.client.uncertain.store(false, Ordering::Release);
            delivery.armed = false;
        }
        Ok(output)
    }
}

/// Real control plus exact-SID writer admission factory; no retirement shortcut.
pub struct NativeSessionPort {
    queue: NativeCommandQueue,
    control: NativeSessionEndpoint,
}
impl NativeSessionPort {
    pub(super) fn open(queue: NativeCommandQueue) -> Result<Self, AgentError> {
        let response = queue.submit(AgentCommand::OpenNativeSession)?.wait()?;
        let authority = match &response {
            AgentReply::NativeEndpoint(authority) => authority.clone(),
            _ => return Err(AgentError::Unknown),
        };
        let control = NativeSessionEndpoint::new(queue.clone(), authority)?;
        Ok(Self { queue, control })
    }
    pub fn control(&self) -> NativeSessionEndpoint {
        self.control.clone()
    }
    pub fn admit_writer(&self, session_id: &str) -> Result<NativeSessionEndpoint, AgentError> {
        if !self.control.client.live() {
            return Err(AgentError::Session(Error::Denied));
        }
        let response = self
            .queue
            .submit(AgentCommand::NativeSessionWriter {
                session_id: session_id.to_owned(),
            })?
            .wait()?;
        let authority = match &response {
            AgentReply::NativeEndpoint(authority) => authority.clone(),
            _ => return Err(AgentError::Unknown),
        };
        NativeSessionEndpoint::new(self.queue.clone(), authority)
    }
    pub fn retire(&self, _session_id: &str) -> Result<(), AgentError> {
        Err(AgentError::Unsupported)
    }
    pub fn now_millis(&self) -> u64 {
        self.queue.now_millis()
    }
}
