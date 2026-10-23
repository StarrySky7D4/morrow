//! Separate consume-only pre-dispatch refusal. Normal terminal ACK and native
//! completion paths are unchanged; no Unknown owner can enter this transaction.
use super::*;
use crate::asuser_a1_native::NativeCallerRefusal;
use crate::station_broker::{CommitClock,StationBrokerConnectRelease,budget_available,take_timely};

pub(crate) struct PendingCallerRefusal {
    owner:Arc<CheckedRunnerOwner>,
    proof:Option<NativeCallerRefusal>,
    binding:Option<Arc<crate::AsUserFactoryBinding>>,
    returned:bool,
}
impl fmt::Debug for PendingCallerRefusal {
    fn fmt(&self,f:&mut fmt::Formatter<'_>)->fmt::Result {f.write_str("PendingCallerRefusal([ORIGINAL_CALLER_ACK_REQUIRED])")}
}
impl fmt::Display for PendingCallerRefusal {
    fn fmt(&self,f:&mut fmt::Formatter<'_>)->fmt::Result {f.write_str("A1 native start refused before dispatch; original refusal return pending")}
}
impl std::error::Error for PendingCallerRefusal {}
impl Drop for PendingCallerRefusal {
    fn drop(&mut self) {
        if !self.returned {self.owner.seal_unknown(StickyUnknownReason::CallerAwaitCancelled);}
    }
}
/// Holds the original cell and closed caller allocation until the same factory
/// consumes it. This alone is not SDK, factory, account or setup retirement.
pub(crate) struct LocalCallerRefusal {
    owner:Arc<CheckedRunnerOwner>,proof:NativeCallerRefusal,
}
impl LocalCallerRefusal {
    pub(crate) fn startup(&self)->u64 {self.owner.startup_id.0}
    pub(crate) fn reason(&self)->io::Result<crate::AsUserRefusalReason> {
        self.require_original_retired()?;
        self.owner.asuser_a1.get().ok_or_else(||io::Error::other("A1 original refusal missing"))?
            .with(|original|original.refusal_reason(&self.proof))
    }
    pub(crate) fn require_original_retired(&self)->io::Result<()> {
        if !self.owner.native_local_retired.load(std::sync::atomic::Ordering::Acquire) {
            return Err(io::Error::other("A1 original refusal is not retired"));
        }
        {let state=self.owner.state.try_lock().map_err(|_|io::Error::other("A1 original refusal owner unavailable"))?;
            require_pristine(&self.owner,&state)?;}
        self.owner.asuser_a1.get().ok_or_else(||io::Error::other("A1 original refusal missing"))?
            .with(|original|if original.caller_refusal_retired(&self.proof){Ok(())}
                else{Err(io::Error::other("A1 original refusal receipt mismatch"))})
    }
}

fn require_pristine(owner:&CheckedRunnerOwner,state:&OwnerState)->io::Result<()> {
    if state.stage!=OwnerLifecycleStage::ReservedBeforeLaunch
        || state.process.is_some() || state.identity.is_some() || state.identity_failure.is_some()
        || state.thread_close_debt.is_some() || state.thread_disposition.is_some()
        || state.helper_attempts.iter().any(Option::is_some) || state.actor_adopted
        || state.worker_admission_active || state.no_replay_seal || state.sticky_unknown.is_some()
        || state.first_error_fact.is_some() || state.hello.is_some() || state.negotiated_handshake.is_some()
        || state.startup_pipe_write.is_some() || state.startup_pipe_read.is_some()
        || state.finalization_in_progress || state.finalized_outcome.is_some()
        || !matches!(state.reader_worker,WorkerSlotState::NotInstalled)
        || !matches!(state.writer_worker,WorkerSlotState::NotInstalled)
        || !matches!(state.input_worker,WorkerSlotState::NotInstalled)
        || state.reader_thread_anchor.is_some() || state.writer_thread_anchor.is_some()
        || state.writer_wake.is_some() || state.owner_mutex_poison_observed
        || state.controls_correlation_poison_observed || state.partial_actor_startup_aborted
        || owner.station_broker.get().is_some() || owner.native_close_runtime.get().is_some()
        || owner.native_desktop.get().is_some() || owner.native_startup_writer.get().is_some()
        || owner.native_startup_reader.get().is_some()
        || !state.artifact.as_ref().is_some_and(|pin|pin.native_close_v8()) {
        return Err(io::Error::other("A1 caller refusal does not own a pristine reservation"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::asuser_a1_native::CallerAdmission;
    use sha2::{Digest,Sha256};
    fn artifact()->Arc<MatchedRunnerArtifact> {
        let exe=std::env::current_exe().unwrap();
        let hash=Sha256::digest(std::fs::read(&exe).unwrap()).into();
        // Actual executable file pin; this fixture never launches it as R.
        Arc::new(MatchedRunnerArtifact::acquire(&exe,hash).unwrap()).native_close_view()
    }
    fn observe(mut reservation:CheckedStartupReservation)->PendingCallerRefusal {
        reservation.reserve_asuser_a1().unwrap();
        let proof=match reservation.owner.with_asuser_a1(|original|original.take_caller_admission()).unwrap() {
            CallerAdmission::Refused(proof)=>proof,
            CallerAdmission::Ready(_)=>panic!("readonly fixture requires an actually missing caller privilege"),
        };
        reservation.pending_caller_refusal_inner(None,proof).unwrap()
    }
    fn scoped(pin:Arc<MatchedRunnerArtifact>)->(CheckedStartupReservation,&'static CheckedOwnerRegistry) {
        let registry=Box::leak(Box::new(CheckedOwnerRegistry::new()));
        let connect=Box::leak(Box::new(owned_connect_worker::Owner::new()));
        (reserve_in_registry(registry,Some(connect),pin,None,Duration::from_secs(30)).unwrap(),registry)
    }
    #[test]
    #[ignore="explicit read-only native caller and production local-registry qualification"]
    fn readonly_native_seventeen_refusals_reuse_original_registry_and_retain_uncertainty() {
        assert_eq!(std::env::var("MORROW_CALLER_REFUSAL_NATIVE_TEST").as_deref(),Ok("1"));
        let destination=std::env::var_os("MORROW_CALLER_REFUSAL_NATIVE_REPORT").expect("fresh evidence path");
        let pin=artifact();
        let runtime=tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
        assert_eq!(global_registry().active_count().unwrap(),0);
        let mut originals=Vec::new();let mut records=Vec::new();
        for _ in 0..17 {
            let reservation=reserve_checked_startup_before_launch(pin.clone(),None).unwrap();
            let caller=reservation.caller_guard();
            let startup=reservation.startup_id().0;
            let pending=runtime.block_on(runtime.spawn_blocking(move||observe(reservation))).unwrap();
            assert_eq!(global_registry().active_count().unwrap(),1);
            let snapshot=pending.owner.with_asuser_a1(|original|Ok(original.caller_refusal_snapshot().unwrap())).unwrap();
            let closed=caller.consume_caller_refusal(pending,&mut CommitClock::new()).unwrap();
            closed.require_original_retired().unwrap();
            assert_eq!(closed.startup(),startup);
            assert_eq!(global_registry().active_count().unwrap(),0);
            records.push(serde_json::json!({"startup":startup,"caller":snapshot,"registry_before":1,"registry_after":0}));
            originals.push(closed);
        }
        assert!(records.windows(2).all(|pair|pair[0]["startup"].as_u64().unwrap()<pair[1]["startup"].as_u64().unwrap()));
        // Negative controls use separate actual original registries. They are
        // never reset and do not contribute to the >16 production-registry run.
        let mut negatives=Vec::new();
        for case in ["caller_cancelled","already_unknown","metadata_expired","registry_busy",
            "registry_post_commit_late","registry_commit_unwind","foreign_caller"] {
            let (reservation,registry)=scoped(pin.clone());
            let owner=reservation.owner.clone();let caller=reservation.caller_guard();
            let pending=runtime.block_on(runtime.spawn_blocking(move||observe(reservation))).unwrap();
            let early=Instant::now();let late=owner.startup_deadline;
            match case {
                "caller_cancelled"=>{drop(caller);drop(pending);},
                "already_unknown"=>{
                    owner.seal_unknown(StickyUnknownReason::CallerAwaitCancelled);
                    assert!(caller.consume_caller_refusal(pending,&mut CommitClock::new()).is_err());
                },
                "metadata_expired"=>assert!(caller.consume_caller_refusal(pending,&mut CommitClock::scripted([late;8],None)).is_err()),
                "registry_busy"=>{
                    let busy=registry.inner.lock().unwrap();
                    assert!(caller.consume_caller_refusal(pending,&mut CommitClock::new()).is_err());drop(busy);
                },
                "registry_post_commit_late"=>assert!(caller.consume_caller_refusal(pending,
                    &mut CommitClock::scripted([early,early,early,early,late,late,late,late],None)).is_err()),
                "registry_commit_unwind"=>assert!(caller.consume_caller_refusal(pending,
                    &mut CommitClock::scripted([early;8],Some(4))).is_err()),
                "foreign_caller"=>{
                    let (foreign,foreign_registry)=scoped(pin.clone());
                    assert!(foreign.caller_guard().consume_caller_refusal(pending,&mut CommitClock::new()).is_err());
                    drop(caller);drop(foreign);assert_eq!(foreign_registry.active_count().unwrap(),1);
                },
                _=>unreachable!(),
            }
            assert!(!owner.native_local_retired.load(std::sync::atomic::Ordering::Acquire));
            // Read the poisoned mutex only to record test facts, never repair it.
            let state=registry.inner.lock().unwrap_or_else(|poisoned|poisoned.into_inner());
            assert_eq!(state.slots.iter().filter(|slot|slot.is_some()).count(),1);
            assert!(state.slots.iter().flatten().any(|entry|Arc::ptr_eq(entry,&owner)));
            negatives.push(serde_json::json!({"case":case,"same_original_retained":true,"capacity_retired":false}));
        }
        for original in &originals {original.require_original_retired().unwrap();}
        let report=serde_json::json!({"native_refusals":17,"actual_checked_token_closes":34,
            "registry":"one original production CHECKED_OWNER_REGISTRY, never reset",
            "original_cells_retained":originals.len(),"process_id":std::process::id(),
            "blocking_returns_acknowledged":17,"records":records,"negative_controls":negatives,
            "runner_starts":0,"account_mutations":0,"policy_mutations":0,
            "local_registry_capacity_qualification":true,"SDK_capacity_qualification":false,"full_P2_accepted":false});
        use std::io::Write;
        let mut file=std::fs::OpenOptions::new().create_new(true).write(true).open(destination).unwrap();
        file.write_all(serde_json::to_vec_pretty(&report).unwrap().as_slice()).unwrap();file.sync_all().unwrap();
    }
}
impl CheckedStartupReservation {
    pub(crate) fn pending_caller_refusal(&mut self,binding:Arc<crate::AsUserFactoryBinding>,proof:NativeCallerRefusal)
        ->io::Result<PendingCallerRefusal> {
        self.pending_caller_refusal_inner(Some(binding),proof)
    }
    fn pending_caller_refusal_inner(&mut self,binding:Option<Arc<crate::AsUserFactoryBinding>>,proof:NativeCallerRefusal)
        ->io::Result<PendingCallerRefusal> {
        if self.committed_to_transport || self.broker_resource_only_retired || self.caller_refusal_pending {
            return Err(io::Error::other("A1 refusal reservation already consumed"));
        }
        self.owner.with_asuser_a1(|original|original.require_caller_refusal(&proof))?;
        {let state=self.owner.state.try_lock().map_err(|_|io::Error::other("A1 refusal owner unavailable"))?;
            require_pristine(&self.owner,&state)?;}
        self.caller_refusal_pending=true;
        Ok(PendingCallerRefusal {owner:self.owner.clone(),proof:Some(proof),binding,returned:false})
    }
}
impl CheckedBlockingCallerGuard {
    pub(crate) fn acknowledge_caller_refusal(self,pending:PendingCallerRefusal)->io::Result<()> {
        let binding=pending.binding.as_ref().ok_or_else(||io::Error::other("A1 refusal factory missing"))?.clone();
        let original=self.consume_caller_refusal(pending,&mut CommitClock::new())?;
        binding.retain_caller_refusal(original)
    }
    fn consume_caller_refusal(mut self,mut pending:PendingCallerRefusal,clock:&mut CommitClock)
        ->io::Result<LocalCallerRefusal> {
        if !Arc::ptr_eq(&self.owner,&pending.owner) {
            return Err(io::Error::other("A1 refusal returned to a different original caller"));
        }
        let proof=pending.proof.as_ref().ok_or_else(||io::Error::other("A1 refusal already consumed"))?;
        let owner=&self.owner;
        let deadline=owner.startup_deadline;
        owner.with_asuser_a1(|original|{
            original.require_caller_refusal(proof)?;
            {let state=owner.state.try_lock().map_err(|_|io::Error::other("A1 refusal owner unavailable"))?;
                require_pristine(owner,&state)?;}
            if !budget_available(deadline,clock) {return Err(io::Error::from(io::ErrorKind::TimedOut));}
            let released=owner.connect_owner.release_startup_reservation_strict(owner.startup_id,deadline,clock);
            if released!=StationBrokerConnectRelease::ReleasedSameStartupId {
                return Err(io::Error::other("A1 original connect reservation not retired"));
            }
            let registry=owner.registry.ok_or_else(||io::Error::other("A1 original registry missing"))?;
            // Only try-locks and finite metadata operations occur in the commit
            // region. Holding the owner state across the registry commit makes
            // a concurrent cancellation seal observable before capacity is freed.
            // Neither lock is waited for and no native API, callback or Drop runs.
            let removed={
                let mut entries=registry.inner.try_lock().map_err(|_|io::Error::other("A1 refusal registry unavailable"))?;
                let state=owner.state.try_lock().map_err(|_|io::Error::other("A1 refusal owner unavailable"))?;
                require_pristine(owner,&state)?;
                let slot=entries.slots.iter_mut().find(|slot|slot.as_ref().is_some_and(|entry|
                    Arc::ptr_eq(entry,owner) && entry.startup_id==owner.startup_id))
                    .ok_or_else(||io::Error::other("A1 refusal registry original mismatch"))?;
                let removed=take_timely(slot,deadline,clock).map_err(|_|io::Error::from(io::ErrorKind::TimedOut))?;
                original.commit_caller_refusal();
                owner.native_local_retired.store(true,std::sync::atomic::Ordering::Release);
                removed
            };
            drop(removed); // pending and this guard still anchor the same cell
            Ok(())
        })?;
        self.acknowledged=true;
        pending.returned=true;
        Ok(LocalCallerRefusal {owner:owner.clone(),proof:pending.proof.take().expect("original held")})
    }
}
