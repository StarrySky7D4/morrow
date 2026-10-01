//! Shared trusted-controller lifetime. No business authority or replay is granted.
use crate::job::{ProcessWatch, terminate_current_process};
use std::{
    io,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU8, Ordering},
    },
    time::{Duration, Instant},
};
#[derive(Clone)]
pub struct LeaseSignal {
    pulse: Arc<Mutex<Instant>>,
    terminal: Arc<AtomicU8>,
    lost_at: Arc<Mutex<Option<Instant>>>,
}
impl Default for LeaseSignal {
    fn default() -> Self {
        Self {
            pulse: Arc::new(Mutex::new(Instant::now())),
            terminal: Arc::new(AtomicU8::new(0)),
            lost_at: Arc::new(Mutex::new(None)),
        }
    }
}
impl LeaseSignal {
    pub fn lost(&self) -> bool {
        self.terminal.load(Ordering::Acquire) == 1
            || self.lost_at.lock().map(|v| v.is_some()).unwrap_or(true)
    }
    pub fn disconnect(&self) {
        // Linearize loss before normal-proof admission without holding any I/O lock.
        let _ = self.terminal.compare_exchange(0, 1, Ordering::AcqRel, Ordering::Acquire);
        if let Ok(mut at) = self.lost_at.lock() {
            at.get_or_insert_with(Instant::now);
        }
    }
    /// Admit exactly one already-complete normal resource proof, before first loss.
    /// The caller must already have closed the business gate and proved original
    /// normal host exit, empty tree, both EOFs and all I/O reclamation. This CAS
    /// does not itself prove resources or durable owner release. A later loss
    /// still revokes authority and starts the independent hard deadline.
    pub fn admit_normal_completion(&self) -> bool {
        self.terminal.compare_exchange(0, 2, Ordering::AcqRel, Ordering::Acquire).is_ok()
    }
    pub fn heartbeat(&self) -> io::Result<()> {
        if self.lost() {
            return Err(io::Error::other("controller lease lost"));
        }
        *self
            .pulse
            .lock()
            .map_err(|_| io::Error::other("lease poison"))? = Instant::now();
        if self.lost() {
            return Err(io::Error::other("controller lease lost"));
        }
        Ok(())
    }
    pub fn lost_elapsed(&self) -> Option<Duration> {
        self.lost_at
            .lock()
            .ok()
            .and_then(|at| at.map(|v| v.elapsed()))
    }
}
pub struct ControllerLease {
    signal: LeaseSignal,
    stop: Arc<AtomicBool>,
    guard_stop: Arc<AtomicBool>,
}
impl ControllerLease {
    pub fn start(
        signal: LeaseSignal,
        pids: &[u32],
        timeout: Duration,
        on_loss: Arc<dyn Fn() + Send + Sync>,
    ) -> io::Result<Self> {
        if pids.is_empty() || pids.len() > 2 || !(100..=5000).contains(&timeout.as_millis()) {
            return Err(io::Error::other("controller watch configuration"));
        }
        let watches = pids
            .iter()
            .map(|p| ProcessWatch::open(*p))
            .collect::<io::Result<Vec<_>>>()?;
        let stop = Arc::new(AtomicBool::new(false));
        let guard_stop = Arc::new(AtomicBool::new(false));
        let stopped = stop.clone();
        let observed = signal.clone();
        std::thread::Builder::new()
            .name("trusted-controller-lease".into())
            .spawn(move || {
                while !stopped.load(Ordering::SeqCst) {
                    if watches.iter().any(|p| p.is_exited().unwrap_or(true))
                        || observed
                            .pulse
                            .lock()
                            .map(|p| p.elapsed() >= timeout)
                            .unwrap_or(true)
                        || observed.lost()
                    {
                        observed.disconnect();
                        on_loss();
                    }
                    std::thread::sleep(Duration::from_millis(5));
                }
            })?;
        let guarded = signal.clone();
        let cancelled = guard_stop.clone();
        if let Err(e) = std::thread::Builder::new()
            .name("trusted-controller-hard-deadline".into())
            .spawn(move || {
                while !cancelled.load(Ordering::SeqCst) {
                    if guarded
                        .lost_elapsed()
                        .is_some_and(|v| v >= Duration::from_secs(5))
                    {
                        terminate_current_process(2);
                    }
                    std::thread::sleep(Duration::from_millis(5));
                }
            })
        {
            stop.store(true, Ordering::SeqCst);
            return Err(e);
        }
        Ok(Self {
            signal,
            stop,
            guard_stop,
        })
    }
}
impl Drop for ControllerLease {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if !self.signal.lost() {
            self.guard_stop.store(true, Ordering::SeqCst);
        }
    }
}

#[cfg(test)]
mod terminal_admission_tests {
    use super::*;
    #[test]
    fn prior_loss_denies_completion_and_renewal() {
        let signal = LeaseSignal::default();
        signal.disconnect();
        assert!(!signal.admit_normal_completion());
        assert!(signal.lost_elapsed().is_some());
        assert!(signal.heartbeat().is_err());
    }
    #[test]
    fn completed_proof_does_not_cancel_later_loss_deadline() {
        let signal = LeaseSignal::default();
        assert!(signal.admit_normal_completion());
        assert!(!signal.admit_normal_completion());
        signal.disconnect();
        assert!(signal.lost());
        assert!(signal.lost_elapsed().is_some());
        assert!(signal.heartbeat().is_err());
        assert_eq!(signal.terminal.load(Ordering::Acquire), 2);
    }
    #[test]
    fn concurrent_loss_and_proof_have_one_atomic_winner() {
        for _ in 0..128 {
            let signal = LeaseSignal::default();
            let barrier = Arc::new(std::sync::Barrier::new(2));
            let lost = signal.clone();
            let ready = barrier.clone();
            let thread = std::thread::spawn(move || { ready.wait(); lost.disconnect(); });
            barrier.wait();
            let admitted = signal.admit_normal_completion();
            thread.join().unwrap();
            assert_eq!(signal.terminal.load(Ordering::Acquire), if admitted { 2 } else { 1 });
            assert!(signal.lost_elapsed().is_some());
            assert!(signal.heartbeat().is_err());
        }
    }
}
