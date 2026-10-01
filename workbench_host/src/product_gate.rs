//! One host admission fence plus cancellation of its original worker/listener.
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};
#[derive(Clone, Default)]
pub struct ProductGate {
    closed: Arc<AtomicBool>,
    stop: Arc<Mutex<Option<Arc<dyn Fn() + Send + Sync>>>>,
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
        if let Some(stop) = stop {
            stop();
        }
    }
    pub fn register(&self, stop: Arc<dyn Fn() + Send + Sync>) -> crate::Result<()> {
        let mut current = self.stop.lock().unwrap_or_else(|e| e.into_inner());
        *current = Some(stop.clone());
        drop(current);
        if self.closed() {
            stop();
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
}
