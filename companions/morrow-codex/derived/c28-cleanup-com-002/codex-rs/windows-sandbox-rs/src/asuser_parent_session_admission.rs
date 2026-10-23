//! Metadata admission for the consume-only genuine parent session.
//! OriginalAttemptRetired is private to the authenticated parent module; only
//! its exact typed native/SDK/factory/setup retirement path supplies one.
//! Common P/C native handles remain charged to the parent until final close.
use std::sync::{Arc, Mutex};

const ACTIVE_LIMIT: usize = 16;
const HISTORY_LIMIT: usize = 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Error { Limit, Foreign, Unknown, Finished }

struct Identity;
use super::{OriginGuard, OriginalAttemptRetired};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Status { Reserved, Running, Closed, Unknown }

struct Slot {
    identity: Arc<Identity>,
    original: Option<Arc<OriginGuard>>,
    status: Status,
}
struct State { history: Vec<Slot>, active: usize, finishing: bool }
pub(super) struct Ledger { state: Mutex<State> }
pub(super) struct Reservation { pub(super) ledger: Arc<Ledger>, identity: Arc<Identity>, sequence: usize }

impl Ledger {
    pub(super) fn new() -> Arc<Self> {
        Arc::new(Self { state: Mutex::new(State { history: Vec::new(), active: 0, finishing: false }) })
    }

    pub(super) fn reserve(self: &Arc<Self>) -> Result<Reservation, Error> {
        let mut state = self.state.lock().map_err(|_| Error::Unknown)?;
        if state.finishing { return Err(Error::Finished); }
        if state.active >= ACTIVE_LIMIT || state.history.len() >= HISTORY_LIMIT { return Err(Error::Limit); }
        let sequence = state.history.len();
        let identity = Arc::new(Identity);
        state.history.push(Slot { identity: identity.clone(), original: None, status: Status::Reserved });
        state.active += 1;
        Ok(Reservation { ledger: self.clone(), identity, sequence })
    }

    fn with_slot<T>(self: &Arc<Self>, reservation: &Reservation,
        action: impl FnOnce(&mut Slot) -> Result<T, Error>) -> Result<T, Error> {
        if !Arc::ptr_eq(self, &reservation.ledger) { return Err(Error::Foreign); }
        let mut state = self.state.lock().map_err(|_| Error::Unknown)?;
        if state.finishing { return Err(Error::Finished); }
        let slot = state.history.get_mut(reservation.sequence).ok_or(Error::Foreign)?;
        if !Arc::ptr_eq(&slot.identity, &reservation.identity) { return Err(Error::Foreign); }
        action(slot)
    }

    // The native owner must be installed before any possible OS side effect.
    pub(super) fn install(self: &Arc<Self>, reservation: &Reservation, original: Arc<OriginGuard>) -> Result<(), Error> {
        self.with_slot(reservation, |slot| {
            if slot.status != Status::Reserved || slot.original.is_some() { return Err(Error::Unknown); }
            slot.original = Some(original.clone());
            slot.status = Status::Running;
            Ok(())
        })
    }

    pub(super) fn retain_unknown(self: &Arc<Self>, reservation: &Reservation) -> Result<(), Error> {
        self.with_slot(reservation, |slot| {
            if slot.status == Status::Closed { return Err(Error::Finished); }
            slot.status = Status::Unknown;
            Ok(())
        })
    }

    pub(super) fn accept_original_close(self: &Arc<Self>, reservation: &Reservation,
        proof: OriginalAttemptRetired) -> Result<(), Error> {
        if !Arc::ptr_eq(self, &reservation.ledger) { return Err(Error::Foreign); }
        let original = {
            let mut state = self.state.lock().map_err(|_| Error::Unknown)?;
            if state.finishing { return Err(Error::Finished); }
            let slot = state.history.get_mut(reservation.sequence).ok_or(Error::Foreign)?;
            if !Arc::ptr_eq(&slot.identity, &reservation.identity) { return Err(Error::Foreign); }
            if slot.status != Status::Running { return Err(Error::Unknown); }
            if !slot.original.as_ref().is_some_and(|held| Arc::ptr_eq(held, &proof.original)) {
                return Err(Error::Foreign);
            }
            slot.status = Status::Closed;
            let original = slot.original.take();
            state.active -= 1;
            original
        };
        // Actual owner/proof destruction must not run under admission metadata.
        drop(original);
        drop(proof);
        Ok(())
    }

    pub(super) fn reserve_final_close(&self) -> Result<(), Error> {
        let mut state = self.state.lock().map_err(|_| Error::Unknown)?;
        if state.finishing { return Err(Error::Finished); }
        if state.active != 0 || state.history.is_empty() ||
            state.history.iter().any(|slot| slot.status != Status::Closed || slot.original.is_some()) {
            return Err(Error::Unknown);
        }
        state.finishing = true;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn start(ledger: &Arc<Ledger>) -> (Reservation, Arc<OriginGuard>) {
        let reservation = ledger.reserve().unwrap();
        let original = super::super::tests::guard();
        ledger.install(&reservation, original.clone()).unwrap();
        (reservation, original)
    }

    #[test]
    fn more_than_limit_closed_attempts_keep_history_then_unknowns_fill_the_same_ledger() {
        let ledger = Ledger::new();
        for sequence in 0..33 {
            let (reservation, original) = start(&ledger);
            assert_eq!(reservation.sequence, sequence);
            ledger.accept_original_close(&reservation, OriginalAttemptRetired { original }).unwrap();
        }
        for _ in 0..ACTIVE_LIMIT {
            let (reservation, _) = start(&ledger);
            ledger.retain_unknown(&reservation).unwrap();
        }
        assert!(matches!(ledger.reserve(), Err(Error::Limit)));
        assert_eq!(ledger.reserve_final_close(), Err(Error::Unknown));
        let state = ledger.state.lock().unwrap();
        assert_eq!(state.active, 16);
        assert_eq!(state.history.len(), 49);
        assert!(state.history[..33].iter().all(|slot| slot.status == Status::Closed));
        assert!(state.history[33..].iter().all(|slot| slot.status == Status::Unknown && slot.original.is_some()));
    }

    #[test]
    fn foreign_owner_wrong_reservation_and_late_close_never_release_unknown_charge() {
        let ledger = Ledger::new();
        let other = Ledger::new();
        let (reservation, original) = start(&ledger);
        let (foreign, foreign_original) = start(&other);
        assert_eq!(ledger.accept_original_close(&foreign, OriginalAttemptRetired { original: original.clone() }), Err(Error::Foreign));
        assert_eq!(ledger.accept_original_close(&reservation, OriginalAttemptRetired { original: foreign_original }), Err(Error::Foreign));
        ledger.retain_unknown(&reservation).unwrap();
        assert_eq!(ledger.accept_original_close(&reservation, OriginalAttemptRetired { original }), Err(Error::Unknown));
        assert_eq!(ledger.state.lock().unwrap().active, 1);
        assert_eq!(other.state.lock().unwrap().active, 1);
    }

    #[test]
    fn competing_closes_discharge_exactly_once_without_erasing_the_spent_identity() {
        let ledger = Ledger::new();
        let (reservation, original) = start(&ledger);
        let reservation = Arc::new(reservation);
        let joins: Vec<_> = (0..8).map(|_| {
            let ledger = ledger.clone(); let reservation = reservation.clone(); let original = original.clone();
            std::thread::spawn(move || ledger.accept_original_close(&reservation, OriginalAttemptRetired { original }))
        }).collect();
        let outcomes: Vec<_> = joins.into_iter().map(|join| join.join().unwrap()).collect();
        assert_eq!(outcomes.iter().filter(|outcome| outcome.is_ok()).count(), 1);
        assert_eq!(ledger.state.lock().unwrap().active, 0);
        assert_eq!(ledger.state.lock().unwrap().history.len(), 1);
        ledger.reserve_final_close().unwrap();
        assert!(matches!(ledger.reserve(), Err(Error::Finished)));
        assert_eq!(ledger.reserve_final_close(), Err(Error::Finished));
    }

    #[test]
    fn preinstalled_reservations_and_unknown_owners_cannot_be_dropped_to_recover_capacity() {
        let ledger = Ledger::new();
        let mut retained = Vec::new();
        for _ in 0..ACTIVE_LIMIT { retained.push(ledger.reserve().unwrap()); }
        drop(retained);
        assert!(matches!(ledger.reserve(), Err(Error::Limit)));
        assert_eq!(ledger.reserve_final_close(), Err(Error::Unknown));
        assert_eq!(ledger.state.lock().unwrap().history.len(), ACTIVE_LIMIT);
        assert_eq!(ledger.state.lock().unwrap().active, ACTIVE_LIMIT);
    }

    #[test]
    fn concurrent_reservations_enforce_the_original_active_limit() {
        let ledger = Ledger::new();
        let starts: Vec<_> = (0..64).map(|_| {
            let ledger = ledger.clone();
            std::thread::spawn(move || ledger.reserve().is_ok())
        }).collect();
        let accepted = starts.into_iter().map(|join| join.join().unwrap()).filter(|ok| *ok).count();
        assert_eq!(accepted, ACTIVE_LIMIT);
        assert_eq!(ledger.state.lock().unwrap().active, ACTIVE_LIMIT);
        assert_eq!(ledger.state.lock().unwrap().history.len(), ACTIVE_LIMIT);
    }

    #[test]
    fn lifetime_history_is_never_deleted_to_make_room() {
        let ledger = Ledger::new();
        for _ in 0..HISTORY_LIMIT {
            let (reservation, original) = start(&ledger);
            ledger.accept_original_close(&reservation, OriginalAttemptRetired { original }).unwrap();
        }
        assert_eq!(ledger.state.lock().unwrap().active, 0);
        assert!(matches!(ledger.reserve(), Err(Error::Limit)));
        assert_eq!(ledger.state.lock().unwrap().history.len(), HISTORY_LIMIT);
        ledger.reserve_final_close().unwrap();
    }
}
