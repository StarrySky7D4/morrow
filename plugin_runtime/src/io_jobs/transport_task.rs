//! Supervised native transport; cancellation is not evidence of task exit.
use crate::Cancellation;

pub(super) struct TransportTask<T: Send + 'static> {
    handle: Option<std::thread::JoinHandle<T>>,
    cancel: Cancellation,
}
impl<T: Send + 'static> TransportTask<T> {
    pub(super) fn spawn(
        cancel: Cancellation,
        run: impl FnOnce() -> T + Send + 'static,
    ) -> std::io::Result<Self> {
        let handle = std::thread::Builder::new()
            .name("morrow-transport".into())
            .spawn(run)?;
        Ok(Self {
            handle: Some(handle),
            cancel,
        })
    }
    pub(super) fn poll(&mut self) -> Option<std::thread::Result<T>> {
        if !self.handle.as_ref()?.is_finished() {
            return None;
        }
        Some(self.handle.take()?.join())
    }
}
impl<T: Send + 'static> Drop for TransportTask<T> {
    fn drop(&mut self) {
        if let Some(handle) = self.handle.take() {
            self.cancel.cancel();
            let _ = handle.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        sync::mpsc,
        time::{Duration, Instant},
    };
    const WAIT: Duration = Duration::from_secs(5);

    #[test]
    fn poll_returns_result_once_without_cancelling_consumed_task() {
        let cancel = Cancellation::default();
        let (entered_tx, entered) = mpsc::sync_channel(1);
        let (release, released) = mpsc::sync_channel(1);
        let mut task = TransportTask::spawn(cancel.clone(), move || {
            entered_tx.send(()).unwrap();
            released.recv_timeout(WAIT).unwrap();
            17
        })
        .unwrap();
        entered.recv_timeout(WAIT).unwrap();
        assert!(task.poll().is_none());
        release.send(()).unwrap();
        let deadline = Instant::now() + WAIT;
        loop {
            if let Some(value) = task.poll() {
                assert_eq!(value.unwrap(), 17);
                break;
            }
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(1));
        }
        assert!(task.poll().is_none());
        drop(task);
        assert!(cancel.fault().is_none());
    }

    #[test]
    fn drop_cancels_but_waits_for_real_exit() {
        let cancel = Cancellation::default();
        let (entered_tx, entered) = mpsc::sync_channel(1);
        let (release, released) = mpsc::sync_channel(1);
        let (exited_tx, exited) = mpsc::sync_channel(1);
        let (dropped_tx, dropped) = mpsc::sync_channel(1);
        let task = TransportTask::spawn(cancel.clone(), move || {
            entered_tx.send(()).unwrap();
            released.recv_timeout(WAIT).unwrap();
            exited_tx.send(()).unwrap();
        })
        .unwrap();
        entered.recv_timeout(WAIT).unwrap();
        let outer = std::thread::spawn(move || {
            drop(task);
            dropped_tx.send(()).unwrap();
        });
        let deadline = Instant::now() + WAIT;
        while cancel.fault().is_none() {
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(1));
        }
        assert!(dropped.try_recv().is_err());
        assert!(exited.try_recv().is_err());
        release.send(()).unwrap();
        dropped.recv_timeout(WAIT).unwrap();
        exited.recv_timeout(WAIT).unwrap();
        outer.join().unwrap();
    }
}
