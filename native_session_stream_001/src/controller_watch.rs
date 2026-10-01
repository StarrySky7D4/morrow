//! Trusted-controller lifetime only. It never extends business authority.
use crate::{Result, Session, pipe_driver::Gate};
use morrow_native_pipe_win::controller_lease::{ControllerLease, LeaseSignal};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};
use std::time::Duration;
#[derive(Clone)]
pub struct Registration {
    notified: Arc<AtomicBool>,
    signal: LeaseSignal,
    session: Arc<Mutex<Option<Session>>>,
    gate: Arc<Mutex<Option<Gate>>>,
}
impl Registration {
    pub fn lost(&self) -> bool {
        self.signal.lost()
    }
    pub fn disconnect(&self) {
        self.signal.disconnect();
        if let Ok(g) = self.gate.lock() {
            if let Some(g) = g.as_ref() {
                if let Ok(mut g) = g.lock() {
                    g.revoked = true;
                }
            }
        }
        if let Ok(s) = self.session.lock() {
            if let Some(s) = s.as_ref() {
                if !self.notified.load(Ordering::SeqCst) && s.controller_lost() {
                    self.notified.store(true, Ordering::SeqCst);
                }
            }
        }
    }
    pub(crate) fn bind_gate(&self, gate: Gate) -> Result<()> {
        *self.gate.lock().map_err(|_| "controller binding poison")? = Some(gate);
        if self.lost() {
            self.disconnect();
            return Err("controller lost before launch".into());
        }
        Ok(())
    }
    pub(crate) fn bind_session(&self, session: Session) {
        self.notified.store(false, Ordering::SeqCst);
        if let Ok(mut s) = self.session.lock() {
            *s = Some(session);
        }
        if self.lost() {
            self.disconnect();
        }
    }
    pub async fn reclamation_deadline(&self) {
        loop {
            if self
                .signal
                .lost_elapsed()
                .is_some_and(|v| v >= Duration::from_millis(4900))
            {
                return;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    }
}

pub struct ControllerWatch {
    registration: Registration,
    _lease: ControllerLease,
}
impl ControllerWatch {
    pub fn start(pids: &[u32], timeout: Duration) -> Result<Self> {
        let signal = LeaseSignal::default();
        let registration = Registration {
            notified: Arc::new(AtomicBool::new(false)),
            signal: signal.clone(),
            session: Arc::new(Mutex::new(None)),
            gate: Arc::new(Mutex::new(None)),
        };
        let callback = registration.clone();
        let lease = ControllerLease::start(
            signal,
            pids,
            timeout,
            Arc::new(move || callback.disconnect()),
        )
        .map_err(|e| e.to_string())?;
        Ok(Self {
            registration,
            _lease: lease,
        })
    }
    pub fn registration(&self) -> Registration {
        self.registration.clone()
    }
    pub fn heartbeat(&self) -> Result<()> {
        self.registration
            .signal
            .heartbeat()
            .map_err(|e| e.to_string())
    }
    pub fn lost(&self) -> bool {
        self.registration.lost()
    }
    pub fn disconnect(&self) {
        self.registration.disconnect();
    }
}
