//! One host admission fence plus cancellation of its original worker/listener.
use std::{
    collections::BTreeMap,
    panic::{AssertUnwindSafe, catch_unwind},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
};
const MAX_SCOPED_STOPS: usize = 32;
type Stop = Arc<dyn Fn() + Send + Sync>;
#[derive(Default)]
struct ScopedStops {
    next: u64,
    callbacks: BTreeMap<u64, Stop>,
}
/// One live cancellation subscription. Dropping it removes only this callback;
/// a callback already sampled by revoke may still run and must be idempotent.
pub struct StopRegistration {
    callbacks: Arc<Mutex<ScopedStops>>,
    identity: u64,
}
impl Drop for StopRegistration {
    fn drop(&mut self) {
        self.callbacks
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .callbacks
            .remove(&self.identity);
    }
}
#[derive(Clone, Default)]
pub struct ProductGate {
    closed: Arc<AtomicBool>,
    stop: Arc<Mutex<Option<Arc<dyn Fn() + Send + Sync>>>>,
    scoped: Arc<Mutex<ScopedStops>>,
    stop_failed: Arc<AtomicBool>,
}
impl ProductGate {
    pub fn closed(&self) -> bool {
        self.closed.load(Ordering::Acquire)
    }
    pub fn check(&self) -> crate::Result<()> {
        if self.closed() {
            Err(
                "Workbench controller lost; business outcome is Unknown. No automatic retry."
                    .into(),
            )
        } else {
            Ok(())
        }
    }
    pub fn revoke(&self) {
        self.closed.store(true, Ordering::Release);
        let stop = self.stop.lock().unwrap_or_else(|e| e.into_inner()).clone();
        let scoped: Vec<_> = self
            .scoped
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .callbacks
            .values()
            .cloned()
            .collect();
        if let Some(stop) = stop {
            self.call_stop(&stop);
        }
        for stop in scoped {
            self.call_stop(&stop);
        }
    }
    fn call_stop(&self, stop: &Stop) {
        if catch_unwind(AssertUnwindSafe(|| stop())).is_err() {
            // Keep the admission fence closed and still notify the other owners.
            // A failed callback is never a process-exit or resource-release proof.
            self.stop_failed.store(true, Ordering::Release);
        }
    }
    pub fn stop_failed(&self) -> bool {
        self.stop_failed.load(Ordering::Acquire)
    }
    /// Add a bounded subscription without replacing the existing IO stop source.
    /// Retain its guard until the corresponding worker and cleanup have finished.
    pub fn register_scoped(&self, stop: Stop) -> crate::Result<StopRegistration> {
        let mut scoped = self.scoped.lock().unwrap_or_else(|e| e.into_inner());
        if self.closed() {
            drop(scoped);
            self.call_stop(&stop);
            return Err("Workbench controller lost; cancellation remains pending.".into());
        }
        if scoped.callbacks.len() >= MAX_SCOPED_STOPS {
            return Err("Workbench cancellation subscription budget exhausted.".into());
        }
        let identity = scoped
            .next
            .checked_add(1)
            .ok_or("Cancellation identity exhausted")?;
        scoped.next = identity;
        scoped.callbacks.insert(identity, stop.clone());
        drop(scoped);
        let registration = StopRegistration {
            callbacks: self.scoped.clone(),
            identity,
        };
        if self.closed() {
            self.call_stop(&stop);
            return Err("Workbench controller lost; cancellation remains pending.".into());
        }
        Ok(registration)
    }
    pub fn register(&self, stop: Arc<dyn Fn() + Send + Sync>) -> crate::Result<()> {
        let mut current = self.stop.lock().unwrap_or_else(|e| e.into_inner());
        *current = Some(stop.clone());
        drop(current);
        if self.closed() {
            self.call_stop(&stop);
            return self.check();
        }
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicUsize;
    #[test]
    fn lost_gate_cancels_original_and_late_workers() {
        let gate = ProductGate::default();
        let n = Arc::new(AtomicUsize::new(0));
        let observed = n.clone();
        gate.register(Arc::new(move || {
            observed.fetch_add(1, Ordering::SeqCst);
        }))
        .unwrap();
        gate.revoke();
        assert!(gate.check().is_err());
        assert_eq!(n.load(Ordering::SeqCst), 1);
        let observed = n.clone();
        assert!(
            gate.register(Arc::new(move || {
                observed.fetch_add(1, Ordering::SeqCst);
            }))
            .is_err()
        );
        assert_eq!(n.load(Ordering::SeqCst), 2);
    }
    #[test]
    fn scoped_stop_preserves_primary_and_drops_only_its_own_identity() {
        let gate = ProductGate::default();
        let primary = Arc::new(AtomicUsize::new(0));
        let first = Arc::new(AtomicUsize::new(0));
        let second = Arc::new(AtomicUsize::new(0));
        let observe = primary.clone();
        gate.register(Arc::new(move || {
            observe.fetch_add(1, Ordering::SeqCst);
        }))
        .unwrap();
        let observe = first.clone();
        let first_guard = gate
            .register_scoped(Arc::new(move || {
                observe.fetch_add(1, Ordering::SeqCst);
            }))
            .unwrap();
        let observe = second.clone();
        let second_guard = gate
            .register_scoped(Arc::new(move || {
                observe.fetch_add(1, Ordering::SeqCst);
            }))
            .unwrap();
        drop(first_guard);
        gate.revoke();
        assert_eq!(primary.load(Ordering::SeqCst), 1);
        assert_eq!(first.load(Ordering::SeqCst), 0);
        assert_eq!(second.load(Ordering::SeqCst), 1);
        assert!(gate.closed());
        assert!(!gate.stop_failed());
        drop(second_guard);
    }
    #[test]
    fn failing_primary_stop_still_notifies_scoped_original_owner() {
        let gate = ProductGate::default();
        gate.register(Arc::new(|| panic!("synthetic callback failure")))
            .unwrap();
        let cancelled = Arc::new(AtomicUsize::new(0));
        let observe = cancelled.clone();
        let guard = gate
            .register_scoped(Arc::new(move || {
                observe.fetch_add(1, Ordering::SeqCst);
            }))
            .unwrap();
        gate.revoke();
        assert!(gate.closed());
        assert!(gate.stop_failed());
        assert_eq!(cancelled.load(Ordering::SeqCst), 1);
        drop(guard);
    }
    #[test]
    fn scoped_budget_is_reusable_and_late_registration_stops_without_retention() {
        let gate = ProductGate::default();
        let mut guards: Vec<_> = (0..MAX_SCOPED_STOPS)
            .map(|_| gate.register_scoped(Arc::new(|| {})).unwrap())
            .collect();
        assert!(gate.register_scoped(Arc::new(|| {})).is_err());
        drop(guards.pop());
        guards.push(gate.register_scoped(Arc::new(|| {})).unwrap());
        gate.revoke();
        let count = Arc::new(AtomicUsize::new(0));
        let observe = count.clone();
        assert!(
            gate.register_scoped(Arc::new(move || {
                observe.fetch_add(1, Ordering::SeqCst);
            }))
            .is_err()
        );
        assert_eq!(count.load(Ordering::SeqCst), 1);
        assert_eq!(
            gate.scoped.lock().unwrap().callbacks.len(),
            MAX_SCOPED_STOPS
        );
        drop(guards);
        assert!(gate.scoped.lock().unwrap().callbacks.is_empty());
    }
}
