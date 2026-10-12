// Ordinary synthetic Core/R2 authority and actual std/Tokio threads only.
// This exercises the production AgentWorker, not StateSlot<WorkbenchState>,
// protected storage, a provisioned backend, OS child Start, or an AD31 route.

fn stage33_tracked_fixture() -> (
    OrdinaryOwner,
    CatalogManagedPackage,
    AgentContext,
    Arc<crate::agent_tasks::context_lifecycle::ContextLifecycleLedger>,
) {
    let (owner, package, mut context) = fixture();
    assert!(context.port.is_none() && context.scheduler.is_none());
    assert!(context.native_session_package.is_none() && context.sealed_proposal.is_none());
    let ledger = crate::agent_tasks::context_lifecycle::ContextLifecycleLedger::new(
        owner.runtime.binding(),
    );
    ledger.register(&mut context).unwrap();
    context.lifecycle.as_ref().unwrap().publish(&context).unwrap();
    context.attach_lifecycle().unwrap();
    assert!(ledger.outstanding());
    (owner, package, context, ledger)
}

#[test]
fn tracked_original_worker_completion_requires_actual_reclaim_to_settle_ledger() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let (owner, package, context, ledger) = stage33_tracked_fixture();
    let identity = (&*owner.identity) as *const u64 as usize;
    let binding = owner.runtime.binding();
    let imports = owner.imports.clone();
    let mut worker = spawn(owner, package, context);
    worker.stop();
    let deadline = Instant::now() + Duration::from_secs(5);
    while !worker.join.as_ref().unwrap().is_finished() {
        assert!(Instant::now() < deadline, "ordinary worker did not finish");
        std::thread::sleep(Duration::from_millis(2));
    }
    // is_finished is observed, but the original JoinHandle is still unjoined.
    assert!(worker.join.is_some() && worker.pending_exit.is_none());
    assert!(ledger.outstanding());
    let exit = join(&mut worker); // Production try_reclaim performs join + ticket witness.
    assert!(worker.join.is_none() && worker.pending_exit.is_none());
    assert_eq!(worker.progress().phase, AgentPhase::Joined);
    assert_eq!(identity, (&*exit.owner.identity) as *const u64 as usize);
    assert_eq!(binding, exit.owner.runtime.binding());
    assert!(exit.disconnect.is_ok() && exit.maintenance.is_ok() && exit.cleanup.is_none());
    assert!(!ledger.outstanding());
    assert_eq!(imports.load(Ordering::SeqCst), 0);
}

#[test]
fn tracked_joined_owner_and_ledger_remain_pending_until_scheduler_alias_releases() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let (owner, package, mut context, ledger) = stage33_tracked_fixture();
    let identity = (&*owner.identity) as *const u64 as usize;
    let binding = owner.runtime.binding();
    let (runtime, stopped) = scheduler();
    context.scheduler = Some(
        SchedulerLease::new(runtime.clone(), context.resources.clone()).unwrap(),
    );
    let mut worker = spawn(owner, package, context);
    worker.stop();
    joined_pending(&mut worker); // Actually joins; returns None while alias remains.
    assert!(worker.join.is_none());
    assert!(ledger.outstanding());
    let retained = worker.pending_exit.as_ref().unwrap();
    assert_eq!(identity, (&*retained.owner.identity) as *const u64 as usize);
    assert_eq!(binding, retained.owner.runtime.binding());
    assert_eq!(worker.progress().scheduler, AgentSchedulerPhase::WaitingForReferences);
    assert_eq!(stopped.load(Ordering::SeqCst), 0);
    drop(runtime);
    let exit = join(&mut worker);
    assert_eq!(identity, (&*exit.owner.identity) as *const u64 as usize);
    assert_eq!(binding, exit.owner.runtime.binding());
    assert_eq!(worker.progress().scheduler, AgentSchedulerPhase::Shutdown);
    assert!(stopped.load(Ordering::SeqCst) >= 2);
    assert!(!ledger.outstanding());
}

#[test]
fn tracked_maintenance_repair_retains_ledger_and_confirmed_authority_cleanup() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let (owner, package, context, ledger) = stage33_tracked_fixture();
    let identity = (&*owner.identity) as *const u64 as usize;
    let binding = owner.runtime.binding();
    let failed = owner.fail_finish.clone();
    let imports = owner.imports.clone();
    let proposer = package.shared_connection();
    let executor = context.executor_connection.clone();
    failed.store(true, Ordering::Release);
    let mut worker = spawn(owner, package, context);
    worker.stop();
    let mut exit = join(&mut worker);
    assert!(worker.join.is_none());
    assert!(exit.disconnect.is_ok());
    assert_eq!(exit.maintenance, Err(AgentError::Maintenance));
    assert!(ledger.outstanding());
    let mut cleanup = exit.cleanup.take().expect("exact original cleanup retained");
    assert!(cleanup.package_closed && cleanup.executor_grant_revoked && cleanup.executor_closed);
    assert!(exit.owner.runtime.connection_phase(&proposer).is_err());
    assert!(exit.owner.runtime.connection_phase(&executor).is_err());
    assert_eq!(cleanup.repair(&mut exit.owner), Err(AgentError::Maintenance));
    assert!(ledger.outstanding());
    assert!(cleanup.package_closed && cleanup.executor_grant_revoked && cleanup.executor_closed);
    failed.store(false, Ordering::Release);
    cleanup.repair(&mut exit.owner).unwrap(); // Production repair settles the ticket.
    assert!(!ledger.outstanding());
    assert!(cleanup.package_closed && cleanup.executor_grant_revoked && cleanup.executor_closed);
    assert!(exit.owner.runtime.connection_phase(&proposer).is_err());
    assert!(exit.owner.runtime.connection_phase(&executor).is_err());
    assert_eq!(identity, (&*exit.owner.identity) as *const u64 as usize);
    assert_eq!(binding, exit.owner.runtime.binding());
    assert_eq!(imports.load(Ordering::SeqCst), 0);
    // Historical exit.maintenance remains the real failure, not rewritten success.
    assert_eq!(exit.maintenance, Err(AgentError::Maintenance));
}
