//! Ownership of one original selection, including partial and queued captures.
//! This is a bounded, in-process ledger, never a guest capability or replay key.
use super::{Error, native_windows::NativeFile, selection_path::MAX_DIRECTORY_SEGMENTS};
use crate::io_binding::IoResourceLease;
use std::{
    fs::File,
    sync::{
        Arc, Mutex, MutexGuard,
        atomic::{AtomicU64, Ordering},
    },
    thread::{self, ThreadId},
};

const MAX_OBJECTS: usize = MAX_DIRECTORY_SEGMENTS + 1;

/// Observations from the original owner only. Zero objects is not proof of a
/// capture, a native picker, delivery, worker join, or any additional authority.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DirectoryRetirementSnapshot {
    pub objects: usize,
    pub close_calls: usize,
    pub confirmed_closed: usize,
    pub unknown: usize,
    pub worker_bound: bool,
    pub picker_started: bool,
    pub picker_closed: bool,
}
impl DirectoryRetirementSnapshot {
    pub fn all_closed(self) -> bool {
        self.worker_bound
            && self.unknown == 0
            && self.objects == self.confirmed_closed
            && (!self.picker_started || self.picker_closed)
    }
    fn packed(self) -> u64 {
        self.objects as u64
            | (self.close_calls as u64) << 8
            | (self.confirmed_closed as u64) << 16
            | (self.unknown as u64) << 24
            | u64::from(self.worker_bound) << 32
            | u64::from(self.picker_started) << 33
            | u64::from(self.picker_closed) << 34
    }
    fn unpacked(value: u64) -> Self {
        Self {
            objects: (value & 255) as usize,
            close_calls: ((value >> 8) & 255) as usize,
            confirmed_closed: ((value >> 16) & 255) as usize,
            unknown: ((value >> 24) & 255) as usize,
            worker_bound: value & (1 << 32) != 0,
            picker_started: value & (1 << 33) != 0,
            picker_closed: value & (1 << 34) != 0,
        }
    }
}

struct State {
    worker: Option<ThreadId>,
    picker_started: bool,
    picker_closed: bool,
    picker_unknown: bool,
    objects: [Option<NativeFile>; MAX_OBJECTS],
    leases: [Option<Arc<IoResourceLease>>; MAX_OBJECTS],
    #[cfg(test)]
    lose_next_success: bool,
    #[cfg(test)]
    before_close: Option<Box<dyn FnOnce() + Send>>,
}
impl State {
    fn snapshot(&self) -> DirectoryRetirementSnapshot {
        let mut result = DirectoryRetirementSnapshot {
            worker_bound: self.worker.is_some(),
            picker_started: self.picker_started,
            picker_closed: self.picker_closed,
            unknown: usize::from(self.picker_unknown),
            ..Default::default()
        };
        for object in self.objects.iter().flatten() {
            result.objects += 1;
            result.close_calls += usize::from(object.attempted());
            result.confirmed_closed += usize::from(object.closed());
            result.unknown += usize::from(object.unknown());
        }
        result
    }
    fn release_known_closed(&mut self) -> bool {
        if self.snapshot().all_closed() {
            // This runs only outside original Control/clock guards and only
            // after every actual CloseHandle result for this selection is known.
            self.leases.fill_with(|| None);
            true
        } else {
            false
        }
    }
}

pub(crate) struct Ledger {
    // Box permits final abandonment to retain the exact records and leases
    // without allocating or retrying an ambiguous native close in Drop.
    state: Mutex<Option<Box<State>>>,
    // Readers must not acquire the private native-object lock. In particular,
    // handle.read and admission diagnostics cannot wait behind a native query.
    published: AtomicU64,
}
impl Ledger {
    pub(crate) const OWNED_BYTES: u64 = (size_of::<Self>()
        + size_of::<State>()
        + 2 * size_of::<usize>()
        + MAX_OBJECTS * (size_of::<IoResourceLease>() + 2 * size_of::<usize>()))
        as u64;

    pub(crate) fn new() -> Arc<Self> {
        Arc::new(Self {
            state: Mutex::new(Some(Box::new(State {
                worker: None,
                picker_started: false,
                picker_closed: false,
                picker_unknown: false,
                objects: std::array::from_fn(|_| None),
                leases: std::array::from_fn(|_| None),
                #[cfg(test)]
                lose_next_success: false,
                #[cfg(test)]
                before_close: None,
            }))),
            published: AtomicU64::new(0),
        })
    }
    fn lock(&self) -> MutexGuard<'_, Option<Box<State>>> {
        self.state.lock().unwrap_or_else(|error| error.into_inner())
    }
    pub(crate) fn bind_worker(&self) {
        let mut guard = self.lock();
        let state = guard.as_mut().expect("original ledger is resident");
        if state.worker.is_none() {
            state.worker = Some(thread::current().id());
        }
        self.publish(state);
    }
    pub(crate) fn begin_picker(&self) -> Result<(), Error> {
        let mut guard = self.lock();
        let state = guard.as_mut().expect("original ledger is resident");
        if state.worker != Some(thread::current().id()) || state.picker_started {
            state.picker_unknown = true;
            self.publish(state);
            return Err(Error::NativeCloseUnknown);
        }
        state.picker_started = true;
        self.publish(state);
        Ok(())
    }
    pub(crate) fn finish_picker(&self) -> Result<(), Error> {
        let mut guard = self.lock();
        let state = guard.as_mut().expect("original ledger is resident");
        if state.worker != Some(thread::current().id())
            || !state.picker_started
            || state.picker_closed
            || state.picker_unknown
        {
            state.picker_unknown = true;
            self.publish(state);
            return Err(Error::NativeCloseUnknown);
        }
        state.picker_closed = true;
        self.publish(state);
        state.release_known_closed();
        Ok(())
    }
    pub(crate) fn picker_unknown(&self) {
        let mut guard = self.lock();
        let state = guard.as_mut().expect("original ledger is resident");
        state.picker_unknown = true;
        self.publish(state);
    }
    pub(crate) fn snapshot(&self) -> DirectoryRetirementSnapshot {
        DirectoryRetirementSnapshot::unpacked(self.published.load(Ordering::Acquire))
    }
    fn publish(&self, state: &State) {
        self.published
            .store(state.snapshot().packed(), Ordering::Release);
    }
    pub(crate) fn settle(&self) -> bool {
        self.lock()
            .as_mut()
            .expect("original ledger is resident")
            .release_known_closed()
    }
    pub(crate) fn track(self: &Arc<Self>, file: File) -> TrackedFile {
        self.bind_worker();
        let mut guard = self.lock();
        let state = guard.as_mut().expect("original ledger is resident");
        // Paths are bounded before native opening; each component opens once.
        let index = state
            .objects
            .iter()
            .position(Option::is_none)
            .expect("validated directory component ceiling");
        state.objects[index] = Some(NativeFile::new(file));
        self.publish(state);
        TrackedFile {
            ledger: Arc::clone(self),
            index,
        }
    }
    pub(crate) fn retain_lease(&self, lease: IoResourceLease) -> Arc<IoResourceLease> {
        let lease = Arc::new(lease);
        let mut guard = self.lock();
        let state = guard.as_mut().expect("original ledger is resident");
        let slot = state
            .leases
            .iter_mut()
            .find(|slot| slot.is_none())
            .expect("validated directory resource ceiling");
        *slot = Some(Arc::clone(&lease));
        lease
    }
    fn close(&self, index: usize) {
        let mut guard = self.lock();
        let state = guard.as_mut().expect("original ledger is resident");
        let on_worker = state.worker == Some(thread::current().id());
        #[cfg(test)]
        if on_worker && let Some(before_close) = state.before_close.take() {
            before_close();
        }
        let object = state.objects[index]
            .as_mut()
            .expect("original object is retained");
        if !on_worker {
            object.mark_unknown();
        } else {
            #[cfg(test)]
            let lose = std::mem::take(&mut state.lose_next_success);
            #[cfg(not(test))]
            let lose = false;
            object.close_once(lose);
        }
        self.publish(state);
        state.release_known_closed();
    }
    #[cfg(test)]
    pub(crate) fn lose_next_close_success(&self) {
        self.lock().as_mut().unwrap().lose_next_success = true;
    }
    #[cfg(test)]
    pub(crate) fn before_close(&self, callback: Box<dyn FnOnce() + Send>) {
        self.lock().as_mut().unwrap().before_close = Some(callback);
    }
}
impl Drop for Ledger {
    fn drop(&mut self) {
        let state = self
            .state
            .get_mut()
            .unwrap_or_else(|error| error.into_inner())
            .take();
        if let Some(state) = state
            && (state.snapshot().unknown != 0
                || state.snapshot().objects != state.snapshot().confirmed_closed
                || (state.picker_started && !state.picker_closed))
        {
            // Fail closed even if every caller abandons the failed worker.
            // Keep original records/quota; no handle reconstruction or retry.
            std::mem::forget(state);
        }
    }
}

/// Unique owner of one registered object; only the ledger keeps its native bits.
pub(crate) struct TrackedFile {
    ledger: Arc<Ledger>,
    index: usize,
}
impl TrackedFile {
    pub(crate) fn ledger(&self) -> Arc<Ledger> {
        Arc::clone(&self.ledger)
    }
    pub(crate) fn with<T>(
        &self,
        query: impl FnOnce(&File) -> Result<T, Error>,
    ) -> Result<T, Error> {
        let mut guard = self.ledger.lock();
        let state = guard.as_mut().expect("original ledger is resident");
        let on_worker = state.worker == Some(thread::current().id());
        let object = state.objects[self.index]
            .as_mut()
            .expect("original object is retained");
        if !on_worker {
            object.mark_unknown();
            self.ledger.publish(state);
            return Err(Error::NativeCloseUnknown);
        }
        query(object.file().ok_or(Error::NativeCloseUnknown)?)
    }
}
impl Drop for TrackedFile {
    fn drop(&mut self) {
        self.ledger.close(self.index);
    }
}

#[cfg(test)]
mod picker_tests {
    use super::*;

    #[test]
    fn zero_files_cannot_hide_a_pending_or_unknown_picker() {
        let ledger = Ledger::new();
        ledger.bind_worker();
        ledger.begin_picker().unwrap();
        let pending = ledger.snapshot();
        assert_eq!(pending.objects, 0);
        assert!(pending.picker_started && !pending.picker_closed);
        assert!(!pending.all_closed());
        assert!(!ledger.settle());
        ledger.picker_unknown();
        assert_eq!(ledger.snapshot().unknown, 1);
        assert_eq!(ledger.finish_picker(), Err(Error::NativeCloseUnknown));
        assert!(!ledger.snapshot().all_closed());
    }

    #[test]
    fn one_original_worker_can_complete_once_without_fabricating_files() {
        let ledger = Ledger::new();
        ledger.bind_worker();
        ledger.begin_picker().unwrap();
        ledger.finish_picker().unwrap();
        let completed = ledger.snapshot();
        assert!(completed.all_closed());
        assert_eq!(completed.objects, 0);
        assert_eq!(completed.close_calls, 0);
        assert_eq!(completed.confirmed_closed, 0);
        assert_eq!(ledger.begin_picker(), Err(Error::NativeCloseUnknown));
        assert_eq!(ledger.snapshot().unknown, 1);
        assert!(!ledger.snapshot().all_closed());
    }

    #[test]
    fn another_worker_cannot_complete_the_original_picker() {
        let ledger = Ledger::new();
        ledger.bind_worker();
        ledger.begin_picker().unwrap();
        let other = Arc::clone(&ledger);
        assert_eq!(
            thread::spawn(move || other.finish_picker()).join().unwrap(),
            Err(Error::NativeCloseUnknown)
        );
        assert_eq!(ledger.snapshot().unknown, 1);
        assert!(!ledger.snapshot().picker_closed);
        assert_eq!(ledger.finish_picker(), Err(Error::NativeCloseUnknown));
    }
}
