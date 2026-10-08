// Included in the existing ordinary-owner fixture module, not a replacement owner.
fn native_request(
    endpoint: &super::super::NativeSessionEndpoint,
    id: &str,
    action: Action,
) -> morrow_agent_session_exec_v1_r2::Reply {
    let request = Request::new_for_generation(id, endpoint.generation(), action).unwrap();
    let bytes = endpoint.exchange_once(request.raw()).unwrap();
    morrow_agent_session_exec_v1_r2::Reply::decode_for(&request, &bytes).unwrap()
}

#[test]
fn genuine_native_session_worker_writer_fence_clone_revoke_and_owner_return() {
    fn send_sync<T: Send + Sync + 'static>() {}
    send_sync::<super::super::NativeSessionEndpoint>();
    send_sync::<super::super::NativeSessionPort>();
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let (owner, package, context) = fixture_with_process(false);
    let binding = owner.runtime.binding();
    let exact_connection = package.shared_connection();
    let owner_identity = (&*owner.identity) as *const u64 as usize;
    let mut worker = spawn(owner, package, context);
    let port = worker.native_session_port().unwrap();
    let control = port.control();
    let cloned_control = control.clone();
    assert!(matches!(
        native_request(
            &control,
            "native-create",
            Action::Create {
                session_id: "session".into(),
                parent: None,
                parent_tail: 0,
            }
        )
        .outcome,
        morrow_agent_session_exec_v1_r2::Outcome::Session(_)
    ));
    let denied = Request::new_for_generation(
        "control-not-writer",
        control.generation(),
        Action::OpenWriter {
            session_id: "session".into(),
            expected_epoch: 0,
        },
    )
    .unwrap();
    assert_eq!(
        control.exchange_once(denied.raw()),
        Err(AgentError::Session(
            morrow_agent_session_exec_v1_r2::Error::Denied
        ))
    );
    assert!(matches!(
        port.admit_writer("unapproved"),
        Err(AgentError::Session(
            morrow_agent_session_exec_v1_r2::Error::Denied
        ))
    ));
    let writer = port.admit_writer("session").unwrap();
    assert!(
        matches!(native_request(&writer, "native-writer", Action::OpenWriter {
        session_id: "session".into(), expected_epoch: 0,
    }).outcome, morrow_agent_session_exec_v1_r2::Outcome::Session(info) if info.epoch == 1)
    );
    assert!(
        matches!(native_request(&writer, "native-append", Action::Append {
        session_id: "session".into(), epoch: 1, expected_tail: 0,
        events: vec![morrow_agent_session_exec_v1_r2::Event {
            event_id: "actual-native-event".into(), body: b"opaque\0\xff".to_vec(),
        }],
    }).outcome, morrow_agent_session_exec_v1_r2::Outcome::Session(info) if info.tail == 1)
    );
    let writer_clone = writer.clone();
    writer.revoke().unwrap();
    let before = worker.progress().accepted_commands;
    let append = Request::new_for_generation(
        "revoked-native-append",
        writer.generation(),
        Action::Append {
            session_id: "session".into(),
            epoch: 1,
            expected_tail: 1,
            events: vec![morrow_agent_session_exec_v1_r2::Event {
                event_id: "must-not-appear".into(),
                body: b"wrong".to_vec(),
            }],
        },
    )
    .unwrap();
    assert_eq!(
        writer_clone.exchange_once(append.raw()),
        Err(AgentError::Session(
            morrow_agent_session_exec_v1_r2::Error::Denied
        ))
    );
    assert_eq!(
        worker.progress().accepted_commands,
        before,
        "closed clone enqueued a write"
    );
    let snapshot = native_request(
        &cloned_control,
        "native-read",
        Action::Snapshot {
            session_id: "session".into(),
            after: 0,
            limit: 16,
        },
    );
    assert!(
        matches!(snapshot.outcome, morrow_agent_session_exec_v1_r2::Outcome::Snapshot(s)
        if s.info.tail == 1 && s.events.len() == 1 && s.events[0].event.body == b"opaque\0\xff")
    );
    assert_eq!(port.retire("session"), Err(AgentError::Unsupported));
    worker.stop();
    let exit = join(&mut worker);
    assert!(exit.disconnect.is_ok() && exit.maintenance.is_ok() && exit.cleanup.is_none());
    assert_eq!(exit.owner.runtime.binding(), binding);
    assert_eq!(
        (&*exit.owner.identity) as *const u64 as usize,
        owner_identity
    );
    assert_ne!(
        exit.owner.runtime.connection_phase(&exact_connection),
        Ok(InstancePhase::Ready)
    );
    assert_eq!(
        cloned_control.exchange_once(denied.raw()),
        Err(AgentError::Session(
            morrow_agent_session_exec_v1_r2::Error::Denied
        ))
    );
}

#[test]
fn genuine_native_writer_a_close_preserves_writer_b_and_control() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let (owner, package, context) = fixture_with_process(false);
    let mut worker = spawn(owner, package, context);
    let port = worker.native_session_port().unwrap();
    let control = port.control();
    for sid in ["session", "session-b"] {
        assert!(matches!(
            native_request(
                &control,
                &format!("create-{sid}"),
                Action::Create {
                    session_id: sid.into(),
                    parent: None,
                    parent_tail: 0,
                }
            )
            .outcome,
            morrow_agent_session_exec_v1_r2::Outcome::Session(_)
        ));
    }
    let a = port.admit_writer("session").unwrap();
    let b = port.admit_writer("session-b").unwrap();
    for (writer, sid) in [(&a, "session"), (&b, "session-b")] {
        assert!(matches!(
            native_request(
                writer,
                &format!("open-{sid}"),
                Action::OpenWriter {
                    session_id: sid.into(),
                    expected_epoch: 0,
                }
            )
            .outcome,
            morrow_agent_session_exec_v1_r2::Outcome::Session(_)
        ));
    }
    a.revoke().unwrap();
    assert!(
        matches!(native_request(&b, "b-after-a-close", Action::Append {
        session_id: "session-b".into(), epoch: 1, expected_tail: 0,
        events: vec![morrow_agent_session_exec_v1_r2::Event {
            event_id: "b-event".into(), body: b"B remains the same writer".to_vec(),
        }],
    }).outcome, morrow_agent_session_exec_v1_r2::Outcome::Session(info) if info.tail == 1)
    );
    assert!(
        matches!(native_request(&control, "control-after-a-close", Action::Snapshot {
        session_id: "session-b".into(), after: 0, limit: 16,
    }).outcome, morrow_agent_session_exec_v1_r2::Outcome::Snapshot(s) if s.info.tail == 1)
    );
    worker.stop();
    assert!(join(&mut worker).cleanup.is_none());
}

// These invoke the actual SDK CatalogManagedPackage/SessionExecHost/Core APIs,
// independently of delivery handling. The supplied clock exercises an actual
// final authorization failure after SQLite publication, never a forged Reply.
#[test]
fn genuine_native_unknown_write_cannot_replay_or_refresh_after_read() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let (mut owner, package, context) = fixture_with_process(false);
    let host = context.host.clone();
    let mut lease = package
        .open_native_session(&owner.manager, &owner.runtime, host.clone(), 6)
        .unwrap();
    let control = lease.control();
    let create = Request::new_for_generation(
        "unknown-create",
        control.generation(),
        Action::Create {
            session_id: "session".into(),
            parent: None,
            parent_tail: 0,
        },
    )
    .unwrap();
    let raw = package
        .exchange_native_session(
            &owner.manager,
            &mut owner.runtime,
            &lease,
            &host,
            control.id(),
            create.raw(),
            || 7,
        )
        .unwrap();
    assert!(matches!(
        morrow_agent_session_exec_v1_r2::Reply::decode_for(&create, &raw)
            .unwrap()
            .outcome,
        morrow_agent_session_exec_v1_r2::Outcome::Session(_)
    ));
    let writer = package
        .native_session_writer(
            &owner.manager,
            &owner.runtime,
            &mut lease,
            &host,
            "session",
            7,
        )
        .unwrap();
    let open = Request::new_for_generation(
        "unknown-open",
        writer.generation(),
        Action::OpenWriter {
            session_id: "session".into(),
            expected_epoch: 0,
        },
    )
    .unwrap();
    package
        .exchange_native_session(
            &owner.manager,
            &mut owner.runtime,
            &lease,
            &host,
            writer.id(),
            open.raw(),
            || 7,
        )
        .unwrap();
    let append = Request::new_for_generation(
        "actual-unknown-append",
        writer.generation(),
        Action::Append {
            session_id: "session".into(),
            epoch: 1,
            expected_tail: 0,
            events: vec![morrow_agent_session_exec_v1_r2::Event {
                event_id: "actual-unknown-event".into(),
                body: b"one durable effect".to_vec(),
            }],
        },
    )
    .unwrap();
    let mut samples = 0;
    let raw = package
        .exchange_native_session(
            &owner.manager,
            &mut owner.runtime,
            &lease,
            &host,
            writer.id(),
            append.raw(),
            || {
                samples += 1;
                if samples == 5 { 6 } else { 7 }
            },
        )
        .unwrap();
    assert_eq!(
        samples, 7,
        "the actual R2 precommit/final clock path changed"
    );
    assert!(matches!(
        morrow_agent_session_exec_v1_r2::Reply::decode_for(&append, &raw)
            .unwrap()
            .outcome,
        morrow_agent_session_exec_v1_r2::Outcome::Rejected(
            morrow_agent_session_exec_v1_r2::Error::CommitUnknown
        )
    ));
    let snapshot = Request::new_for_generation(
        "unknown-read-reconcile",
        control.generation(),
        Action::Snapshot {
            session_id: "session".into(),
            after: 0,
            limit: 16,
        },
    )
    .unwrap();
    let raw = package
        .exchange_native_session(
            &owner.manager,
            &mut owner.runtime,
            &lease,
            &host,
            control.id(),
            snapshot.raw(),
            || 7,
        )
        .unwrap();
    assert!(
        matches!(morrow_agent_session_exec_v1_r2::Reply::decode_for(&snapshot, &raw).unwrap().outcome,
        morrow_agent_session_exec_v1_r2::Outcome::Snapshot(s)
            if s.info.tail == 1 && s.events.len() == 1 && s.events[0].event.body == b"one durable effect")
    );
    assert_eq!(
        package.exchange_native_session(
            &owner.manager,
            &mut owner.runtime,
            &lease,
            &host,
            writer.id(),
            append.raw(),
            || 7
        ),
        Err(morrow_agent_session_exec_v1_r2::Error::CommitUnknown),
        "replayed Unknown original frame"
    );
    assert!(matches!(
        package.native_session_writer(
            &owner.manager,
            &owner.runtime,
            &mut lease,
            &host,
            "session",
            7
        ),
        Err(morrow_agent_session_exec_v1_r2::Error::CommitUnknown)
    ));
    assert!(
        matches!(
            package.open_native_session(&owner.manager, &owner.runtime, host.clone(), 7),
            Err(morrow_agent_session_exec_v1_r2::Error::Denied)
        ),
        "new lease refreshed Unknown"
    );
    lease.close().unwrap();
    package.close_session(&mut owner.runtime, &host).unwrap();
    host.revoke(&context.executor_admission).unwrap();
    owner
        .runtime
        .disconnect(&context.executor_connection)
        .unwrap();
}

#[test]
fn genuine_native_concurrent_writer_close_uses_original_admission_without_deadlock() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let (mut owner, package, context) = fixture_with_process(false);
    let host = context.host.clone();
    let mut lease = package
        .open_native_session(&owner.manager, &owner.runtime, host.clone(), 6)
        .unwrap();
    let control = lease.control();
    let create = Request::new_for_generation(
        "race-create",
        control.generation(),
        Action::Create {
            session_id: "session".into(),
            parent: None,
            parent_tail: 0,
        },
    )
    .unwrap();
    package
        .exchange_native_session(
            &owner.manager,
            &mut owner.runtime,
            &lease,
            &host,
            control.id(),
            create.raw(),
            || 7,
        )
        .unwrap();
    let writer = package
        .native_session_writer(
            &owner.manager,
            &owner.runtime,
            &mut lease,
            &host,
            "session",
            7,
        )
        .unwrap();
    let open = Request::new_for_generation(
        "race-open",
        writer.generation(),
        Action::OpenWriter {
            session_id: "session".into(),
            expected_epoch: 0,
        },
    )
    .unwrap();
    package
        .exchange_native_session(
            &owner.manager,
            &mut owner.runtime,
            &lease,
            &host,
            writer.id(),
            open.raw(),
            || 7,
        )
        .unwrap();
    let append = Request::new_for_generation(
        "race-append",
        writer.generation(),
        Action::Append {
            session_id: "session".into(),
            epoch: 1,
            expected_tail: 0,
            events: vec![morrow_agent_session_exec_v1_r2::Event {
                event_id: "race-event".into(),
                body: b"must not publish after precommit revoke".to_vec(),
            }],
        },
    )
    .unwrap();
    let mut samples = 0;
    let mut closing = None;
    let result = package.exchange_native_session(
        &owner.manager,
        &mut owner.runtime,
        &lease,
        &host,
        writer.id(),
        append.raw(),
        || {
            samples += 1;
            if samples == 4 {
                let close = writer.clone();
                closing = Some(std::thread::spawn(move || close.close()));
                let deadline = Instant::now() + Duration::from_secs(3);
                while !writer.is_closed() {
                    assert!(
                        Instant::now() < deadline,
                        "original per-admission revoke did not begin"
                    );
                    std::thread::yield_now();
                }
            }
            7
        },
    );
    assert_eq!(
        result,
        Err(morrow_agent_session_exec_v1_r2::Error::CommitUnknown)
    );
    closing.take().unwrap().join().unwrap().unwrap();
    assert_eq!(
        owner.runtime.connection_phase(&package.shared_connection()),
        Ok(InstancePhase::Ready)
    );
    let snapshot = Request::new_for_generation(
        "race-read",
        control.generation(),
        Action::Snapshot {
            session_id: "session".into(),
            after: 0,
            limit: 16,
        },
    )
    .unwrap();
    let raw = package
        .exchange_native_session(
            &owner.manager,
            &mut owner.runtime,
            &lease,
            &host,
            control.id(),
            snapshot.raw(),
            || 7,
        )
        .unwrap();
    assert!(
        matches!(morrow_agent_session_exec_v1_r2::Reply::decode_for(&snapshot, &raw).unwrap().outcome,
        morrow_agent_session_exec_v1_r2::Outcome::Snapshot(s) if s.info.tail == 0 && s.events.is_empty())
    );
    assert_eq!(
        package.exchange_native_session(
            &owner.manager,
            &mut owner.runtime,
            &lease,
            &host,
            writer.id(),
            append.raw(),
            || 7
        ),
        Err(morrow_agent_session_exec_v1_r2::Error::Denied)
    );
    lease.close().unwrap();
    package.close_session(&mut owner.runtime, &host).unwrap();
    host.revoke(&context.executor_admission).unwrap();
    owner
        .runtime
        .disconnect(&context.executor_connection)
        .unwrap();
}

#[test]
fn genuine_native_session_factory_rejects_process_approval_and_keeps_original_owner() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let (owner, package, context) = fixture(); // actual approval includes process-read
    let binding = owner.runtime.binding();
    let mut worker = spawn(owner, package, context);
    assert!(matches!(
        worker.native_session_port(),
        Err(AgentError::Session(
            morrow_agent_session_exec_v1_r2::Error::Denied
        ))
    ));
    worker.stop();
    let exit = join(&mut worker);
    assert_eq!(exit.owner.runtime.binding(), binding);
    assert!(exit.disconnect.is_ok() && exit.cleanup.is_none());
}

#[test]
fn genuine_native_session_generation_and_shared_command_budget_are_not_renewed() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let (owner, package, context) = fixture_with_process(false);
    let mut worker = spawn(owner, package, context);
    let port = worker.native_session_port().unwrap();
    let control = port.control();
    let before = worker.progress().accepted_commands;
    let wrong =
        Request::new_for_generation("wrong-generation", control.generation() + 1, Action::List)
            .unwrap();
    assert_eq!(
        control.exchange_once(wrong.raw()),
        Err(AgentError::Session(
            morrow_agent_session_exec_v1_r2::Error::Correlation
        ))
    );
    assert_eq!(worker.progress().accepted_commands, before);
    for n in before..16 {
        let request = Request::new_for_generation(
            format!("budget-read-{n}"),
            control.generation(),
            Action::List,
        )
        .unwrap();
        let bytes = control.exchange_once(request.raw()).unwrap();
        morrow_agent_session_exec_v1_r2::Reply::decode_for(&request, &bytes).unwrap();
    }
    let request =
        Request::new_for_generation("no-renewal", control.generation(), Action::List).unwrap();
    assert_eq!(control.exchange_once(request.raw()), Err(AgentError::Limit));
    assert_eq!(worker.progress().accepted_commands, 16);
    control.revoke().unwrap(); // out-of-band original revoke still works at the bound
    worker.stop();
    assert!(join(&mut worker).cleanup.is_none());
}

#[test]
fn genuine_native_catalog_factory_cannot_reopen_after_lease_drop() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let (mut owner, package, context) = fixture_with_process(false);
    let host = context.host.clone();
    let connection = package.shared_connection();
    let lease = package
        .open_native_session(&owner.manager, &owner.runtime, host.clone(), 6)
        .unwrap();
    let control = lease.control();
    drop(lease);
    assert!(control.is_closed());
    assert!(
        matches!(
            package.open_native_session(&owner.manager, &owner.runtime, host.clone(), 6),
            Err(morrow_agent_session_exec_v1_r2::Error::Denied)
        ),
        "drop reset the factory latch"
    );
    assert_eq!(
        owner.runtime.connection_phase(&connection),
        Ok(InstancePhase::Ready)
    );
    package.close_session(&mut owner.runtime, &host).unwrap();
    host.revoke(&context.executor_admission).unwrap();
    owner
        .runtime
        .disconnect(&context.executor_connection)
        .unwrap();
}

#[test]
fn genuine_native_worker_delayed_delivery_checks_original_admission_ttl_and_latches_unknown() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    // Both expiry and a rollback within created/expires must veto delivery.
    for (server_time, delivery_time) in [(6, 31), (7, 6)] {
        let (owner, package, context) = fixture_with_expiry(false, 30);
        let binding = owner.runtime.binding();
        let caller = std::thread::current().id();
        let armed = Arc::new(AtomicBool::new(false));
        let samples = Arc::new(AtomicUsize::new(0));
        let service_time = Arc::new(std::sync::atomic::AtomicU64::new(6));
        let clock = {
            let armed = armed.clone();
            let samples = samples.clone();
            let service_time = service_time.clone();
            Arc::new(move || {
                if std::thread::current().id() == caller && armed.load(Ordering::Acquire) {
                    if samples.fetch_add(1, Ordering::AcqRel) >= 3 {
                        delivery_time
                    } else {
                        6
                    }
                } else {
                    service_time.load(Ordering::Acquire)
                }
            })
        };
        let mut worker = AgentWorker::spawn(
            owner,
            package,
            context,
            ProductGate::default(),
            AgentLimits::default(),
            60_000,
            clock,
        )
        .unwrap_or_else(|_| panic!("ordinary TTL owner spawn failed"));
        let port = worker.native_session_port().unwrap();
        let control = port.control();
        let create = Request::new_for_generation(
            "delivered-after-native-time-veto",
            control.generation(),
            Action::Create {
                session_id: "session".into(),
                parent: None,
                parent_tail: 0,
            },
        )
        .unwrap();
        service_time.store(server_time, Ordering::Release);
        armed.store(true, Ordering::Release);
        assert_eq!(
            control.exchange_once(create.raw()),
            Err(AgentError::Unknown)
        );
        assert_eq!(
            samples.load(Ordering::Acquire),
            4,
            "expected veto at post-receive native check"
        );
        armed.store(false, Ordering::Release);
        // A subsequent valid original sample cannot unlock delivery Unknown.
        let second_port = worker.native_session_port().unwrap();
        let second_control = second_port.control();
        let snapshot = native_request(
            &second_control,
            "ttl-reconcile-read",
            Action::Snapshot {
                session_id: "session".into(),
                after: 0,
                limit: 16,
            },
        );
        assert!(
            matches!(snapshot.outcome, morrow_agent_session_exec_v1_r2::Outcome::Snapshot(s)
            if s.info.tail == 0 && s.events.is_empty()),
            "the actual original-owner Create did not publish"
        );
        assert_eq!(
            second_control.exchange_once(create.raw()),
            Err(AgentError::Session(
                morrow_agent_session_exec_v1_r2::Error::CommitUnknown
            ))
        );
        assert!(matches!(
            second_port.admit_writer("session"),
            Err(AgentError::Session(
                morrow_agent_session_exec_v1_r2::Error::CommitUnknown
            ))
        ));
        worker.stop();
        let exit = join(&mut worker);
        assert_eq!(exit.owner.runtime.binding(), binding);
        assert!(exit.disconnect.is_ok() && exit.maintenance.is_ok() && exit.cleanup.is_none());
    }
}

#[test]
fn genuine_native_session_factory_rejects_same_generation_rotated_r2_issuer() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let (mut owner, package, context) = fixture_with_process(false);
    let host_a = context.host.clone();
    let generation = host_a.generation(&owner.runtime).unwrap();
    let original_connection = package.shared_connection();
    // Actual SessionExecHost::new rotates the original Core ledger owner lease.
    // Persisted R2 metadata generation remains the same; it is not an issuer.
    let host_b = Arc::new(SessionExecHost::new(&mut owner.runtime).unwrap());
    assert_eq!(host_b.generation(&owner.runtime).unwrap(), generation);
    assert_eq!(
        owner.runtime.connection_phase(&original_connection),
        Ok(InstancePhase::Ready)
    );
    let read =
        Request::new_for_generation("rotation-old-admission", generation, Action::List).unwrap();
    for original_host in [&host_a, &host_b] {
        let raw = original_host
            .dispatch(
                &mut owner.runtime,
                &context.executor_connection,
                &context.executor_admission,
                read.raw(),
                || 6,
            )
            .unwrap();
        assert!(matches!(
            morrow_agent_session_exec_v1_r2::Reply::decode_for(&read, &raw)
                .unwrap()
                .outcome,
            morrow_agent_session_exec_v1_r2::Outcome::Rejected(
                morrow_agent_session_exec_v1_r2::Error::Denied
            )
        ));
    }
    assert!(
        matches!(
            package.open_native_session(&owner.manager, &owner.runtime, host_b, 6),
            Err(morrow_agent_session_exec_v1_r2::Error::Denied)
        ),
        "same generation/new issuer minted native authority from the old approval"
    );
    // Failure is pre-admission and cannot consume a fresh native factory grant.
    // Cleanup uses retained original host/admission and the actual same Core.
    package.close_session(&mut owner.runtime, &host_a).unwrap();
    host_a.revoke(&context.executor_admission).unwrap();
    owner
        .runtime
        .disconnect(&context.executor_connection)
        .unwrap();
    assert_ne!(
        owner.runtime.connection_phase(&original_connection),
        Ok(InstancePhase::Ready)
    );
}
