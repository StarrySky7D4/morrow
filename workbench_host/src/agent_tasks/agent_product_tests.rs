// Included only inside the existing worker.rs ordinary-owner fixture module.
// Real original AgentWorker + session lease + isolated temporary SQLite/catalog.
// No protected Workbench, provisioned backend, native Start, VM or DPAPI.
mod product_controller_tests {
    use super::*;
    use crate::agent_tasks::agent_product::{
        DispatchObserver, ProductSessionController, SessionCompletion, SessionPoll,
    };
    use crate::io_tasks::TaskKey;
    use morrow_agent_session_exec_v1_r2::{Outcome, Reply};

    #[derive(Default)]
    struct DispatchProbe {
        enqueues: AtomicUsize,
        start_callbacks: AtomicUsize,
    }
    impl DispatchObserver for DispatchProbe {
        fn enqueued(&self, command: &AgentCommand) {
            self.enqueues.fetch_add(1, Ordering::SeqCst);
            match command {
                AgentCommand::OpenNativeSession | AgentCommand::NativeSessionWriter { .. }
                | AgentCommand::NativeSessionExchange { .. } => {}
                AgentCommand::StartClaimed { .. } => {
                    self.start_callbacks.fetch_add(1, Ordering::SeqCst);
                    panic!("product controller reached forbidden Start dispatch");
                }
                _ => panic!("product controller reached a non-session command"),
            }
        }
    }
    // Counts forbidden dispatch callbacks, not a fictitious native provider
    // invocation. The actual fixture has port=None; no backend is constructed.
    fn callback_zero(probe: &DispatchProbe) {
        assert_eq!(probe.start_callbacks.load(Ordering::SeqCst), 0);
    }
    fn controller(
        worker: &AgentWorker<OrdinaryOwner>, binding: HostBinding,
    ) -> (ProductSessionController, Arc<DispatchProbe>) {
        let key = TaskKey::from_bytes(&[7; 32]).unwrap();
        let mut controller = ProductSessionController::from_original(
            key, binding, worker.native_command_queue(),
        );
        let probe = Arc::new(DispatchProbe::default());
        controller.observe_dispatch(probe.clone());
        assert_eq!(controller.owner_binding(), binding);
        assert_eq!(controller.status().task, key);
        (controller, probe)
    }
    fn complete(
        controller: &mut ProductSessionController,
        submission: crate::agent_tasks::agent_product::Submission,
    ) -> Result<SessionCompletion, AgentError> {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            match controller.poll(submission)? {
                SessionPoll::Complete(value) => return Ok(value),
                SessionPoll::Pending => {}
            }
            assert!(Instant::now() < deadline, "bounded ordinary command did not return");
            std::thread::sleep(Duration::from_millis(1));
        }
    }
    fn opened(controller: &mut ProductSessionController) -> u64 {
        let submission = controller.open().unwrap();
        assert!(matches!(controller.open(), Err(AgentError::Busy)));
        let generation = match complete(controller, submission).unwrap() {
            SessionCompletion::Opened { generation } => generation,
            _ => panic!("expected original endpoint"),
        };
        assert!(generation > 0);
        assert!(matches!(controller.poll(submission), Err(AgentError::Invalid)));
        generation
    }
    fn create(generation: u64, id: &str) -> Request {
        Request::new_for_generation(id, generation, Action::Create {
            session_id: "session".into(), parent: None, parent_tail: 0,
        }).unwrap()
    }

    #[test]
    fn product_open_create_snapshot_retains_one_handle_and_never_dispatches_start() {
        let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        let (owner, package, context) = fixture_with_process(false);
        assert!(context.port().is_none());
        let binding = owner.runtime.binding();
        let mut worker = spawn(owner, package, context);
        let (mut controller, probe) = controller(&worker, binding);
        let generation = opened(&mut controller);
        assert!(matches!(controller.open(), Err(AgentError::Invalid)));
        let request = create(generation, "product-create");
        let submission = controller.exchange(request.raw()).unwrap();
        assert!(matches!(controller.exchange(request.raw()), Err(AgentError::Busy)));
        assert_eq!(controller.status().pending, Some(submission));
        assert_eq!(controller.status().pending, Some(submission), "status consumed a reply");
        let raw = match complete(&mut controller, submission).unwrap() {
            SessionCompletion::Reply(raw) => raw,
            _ => panic!("expected correlated session reply"),
        };
        assert!(matches!(Reply::decode_for(&request, &raw).unwrap().outcome, Outcome::Session(_)));
        assert!(matches!(controller.poll(submission), Err(AgentError::Invalid)));
        assert!(matches!(controller.exchange(request.raw()), Err(AgentError::Invalid)));
        let snapshot = Request::new_for_generation("product-snapshot", generation,
            Action::Snapshot { session_id: "session".into(), after: 0, limit: 8 }).unwrap();
        let submission = controller.exchange(snapshot.raw()).unwrap();
        let raw = match complete(&mut controller, submission).unwrap() {
            SessionCompletion::Reply(raw) => raw, _ => panic!("expected snapshot"),
        };
        assert!(matches!(Reply::decode_for(&snapshot, &raw).unwrap().outcome,
            Outcome::Snapshot(value) if value.info.tail == 0 && value.events.is_empty()));
        callback_zero(&probe);
        assert_eq!(probe.enqueues.load(Ordering::SeqCst), 3);
        drop(controller);
        worker.stop();
        let exit = join(&mut worker);
        assert_eq!(exit.owner.runtime.binding(), binding);
        assert!(exit.disconnect.is_ok() && exit.maintenance.is_ok() && exit.cleanup.is_none());
    }

    #[test]
    fn product_denies_claim_writer_wrong_generation_and_retirement_before_enqueue() {
        let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        let (owner, package, context) = fixture_with_process(false);
        assert!(context.port().is_none());
        let binding = owner.runtime.binding();
        let mut worker = spawn(owner, package, context);
        let (mut controller, probe) = controller(&worker, binding);
        let generation = opened(&mut controller);
        let before = probe.enqueues.load(Ordering::SeqCst);
        let claim = Request::new_for_generation("forbidden-claim", generation,
            Action::Claim { operation_id: "tool".into(), permit: [1; 32] }).unwrap();
        assert!(matches!(controller.exchange(claim.raw()), Err(AgentError::Unsupported)));
        let writer = Request::new_for_generation("forbidden-writer", generation,
            Action::OpenWriter { session_id: "session".into(), expected_epoch: 0 }).unwrap();
        assert!(matches!(controller.exchange(writer.raw()), Err(AgentError::Unsupported)));
        let wrong = create(generation.checked_add(1).unwrap(), "wrong-generation");
        assert!(matches!(controller.exchange(wrong.raw()),
            Err(AgentError::Session(morrow_agent_session_exec_v1_r2::Error::Correlation))));
        assert!(matches!(controller.retire("session"), Err(AgentError::Unsupported)));
        assert_eq!(probe.enqueues.load(Ordering::SeqCst), before);
        callback_zero(&probe);
        drop(controller);
        worker.stop();
        let exit = join(&mut worker);
        assert_eq!(exit.owner.runtime.binding(), binding);
    }

    #[test]
    fn product_lost_delivery_drains_original_once_but_never_unseals_or_resends() {
        let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        let (owner, package, context) = fixture_with_process(false);
        let binding = owner.runtime.binding();
        let mut worker = spawn(owner, package, context);
        let (mut controller, probe) = controller(&worker, binding);
        let generation = opened(&mut controller);
        let request = create(generation, "lost-product-create");
        let submission = controller.exchange(request.raw()).unwrap();
        controller.mark_delivery_unknown();
        assert!(controller.status().delivery_unknown);
        assert_eq!(controller.status().pending, Some(submission));
        assert!(matches!(complete(&mut controller, submission),
            Err(AgentError::Unknown | AgentError::Session(_))));
        assert!(matches!(controller.poll(submission), Err(AgentError::Invalid)));
        assert!(controller.status().delivery_unknown);
        let before = probe.enqueues.load(Ordering::SeqCst);
        let list = Request::new_for_generation("lost-product-list", generation, Action::List).unwrap();
        assert!(matches!(controller.exchange(list.raw()), Err(AgentError::Unknown)));
        assert!(matches!(controller.open(), Err(AgentError::Unknown)));
        assert_eq!(probe.enqueues.load(Ordering::SeqCst), before);
        callback_zero(&probe);
        drop(controller);
        worker.stop();
        let exit = join(&mut worker);
        assert_eq!(exit.owner.runtime.binding(), binding);
    }

    #[test]
    fn product_drop_pending_open_requests_cancellation_without_claiming_owner_join() {
        let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        let (owner, package, context) = fixture_with_process(false);
        let binding = owner.runtime.binding();
        let mut worker = spawn(owner, package, context);
        let (mut controller, probe) = controller(&worker, binding);
        let submission = controller.open().unwrap();
        assert_eq!(controller.status().pending, Some(submission));
        drop(controller); // Original handle Drop; never a replacement runtime.
        callback_zero(&probe);
        worker.stop();
        let exit = join(&mut worker); // Actual join belongs to the original worker.
        assert_eq!(exit.owner.runtime.binding(), binding);
    }

    #[test]
    fn product_new_controller_cannot_clear_shared_lost_delivery_or_create_again() {
        let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        let (owner, package, context) = fixture_with_process(false);
        assert!(context.port().is_none());
        let binding = owner.runtime.binding();
        let mut worker = spawn(owner, package, context);
        let (mut first, first_probe) = controller(&worker, binding);
        let generation = opened(&mut first);
        let request = create(generation, "first-delivered-create");
        let submission = first.exchange(request.raw()).unwrap();
        let raw = match complete(&mut first, submission).unwrap() {
            SessionCompletion::Reply(raw) => raw, _ => panic!("expected real server reply"),
        };
        assert!(matches!(Reply::decode_for(&request, &raw).unwrap().outcome, Outcome::Session(_)));
        // The trusted outer transport loses that produced reply. This sets the
        // genuine shared server lease fence, not a test-only response fixture.
        first.mark_delivery_unknown();
        assert!(first.status().delivery_unknown);
        let (mut second, second_probe) = controller(&worker, binding);
        assert_eq!(opened(&mut second), generation);
        // Local false cannot attest shared recovery; original reads still work.
        assert!(!second.status().delivery_unknown);
        let snapshot = Request::new_for_generation("second-read", generation,
            Action::Snapshot { session_id: "session".into(), after: 0, limit: 8 }).unwrap();
        let submission = second.exchange(snapshot.raw()).unwrap();
        let raw = match complete(&mut second, submission).unwrap() {
            SessionCompletion::Reply(raw) => raw, _ => panic!("expected original snapshot"),
        };
        assert!(matches!(Reply::decode_for(&snapshot, &raw).unwrap().outcome,
            Outcome::Snapshot(value) if value.info.tail == 0 && value.events.is_empty()));
        // A different approved SID/id excludes duplicate-create conflict.
        let blocked = Request::new_for_generation("second-create", generation,
            Action::Create { session_id: "session-b".into(), parent: None, parent_tail: 0 }).unwrap();
        let submission = second.exchange(blocked.raw()).unwrap();
        assert!(matches!(complete(&mut second, submission),
            Err(AgentError::Session(morrow_agent_session_exec_v1_r2::Error::CommitUnknown))));
        assert!(second.status().delivery_unknown);
        let count = second_probe.enqueues.load(Ordering::SeqCst);
        assert!(matches!(second.exchange(blocked.raw()), Err(AgentError::Unknown)));
        assert_eq!(second_probe.enqueues.load(Ordering::SeqCst), count);
        callback_zero(&first_probe);
        callback_zero(&second_probe);
        drop(second);
        drop(first);
        worker.stop();
        let exit = join(&mut worker);
        assert_eq!(exit.owner.runtime.binding(), binding);
        assert!(exit.disconnect.is_ok() && exit.maintenance.is_ok() && exit.cleanup.is_none());
    }

    #[test]
    fn product_final_delivery_checks_original_endpoint_expiry_and_revocation() {
        use morrow_agent_session_process_v1_host::native_session::NativeEndpointAuthority;
        use std::sync::atomic::AtomicBool;
        let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        for revoke_at_delivery in [false, true] {
            let (owner, package, context) = fixture_with_expiry(false, 30);
            assert!(context.port().is_none());
            let binding = owner.runtime.binding();
            let caller = std::thread::current().id();
            let armed = Arc::new(AtomicBool::new(false));
            let samples = Arc::new(AtomicUsize::new(0));
            let original = Arc::new(Mutex::new(None::<NativeEndpointAuthority>));
            let clock: Arc<dyn Fn() -> u64 + Send + Sync> = {
                let armed = armed.clone();
                let samples = samples.clone();
                let original = original.clone();
                Arc::new(move || {
                    if std::thread::current().id() == caller && armed.load(Ordering::SeqCst) {
                        // ready, enqueue, successful original try_read see 6;
                        // only final delivery sees expiry or original revocation.
                        // The real owner worker always observes 6.
                        let sample = samples.fetch_add(1, Ordering::SeqCst);
                        assert!(sample <= 3, "unexpected caller clock consumption");
                        if sample == 3 {
                            if revoke_at_delivery {
                                let authority = original.lock().unwrap_or_else(|e| e.into_inner())
                                    .as_ref().expect("original server authority retained").clone();
                                authority.close().unwrap();
                            } else { return 31; }
                        }
                    }
                    6
                })
            };
            let mut worker = AgentWorker::spawn(owner, package, context,
                ProductGate::default(), AgentLimits::default(), 60_000, clock)
                .unwrap_or_else(|_| panic!("ordinary owner spawn failed"));
            let (mut product, probe) = controller(&worker, binding);
            let generation = opened(&mut product);
            // Genuine second Open returns the same existing lease control.
            let reply = worker.submit(AgentCommand::OpenNativeSession).unwrap().wait().unwrap();
            let authority = match &reply {
                AgentReply::NativeEndpoint(authority) => authority.clone(),
                _ => panic!("expected original control authority"),
            };
            assert_eq!(authority.generation(), generation);
            assert!(authority.is_live_at(6));
            *original.lock().unwrap_or_else(|e| e.into_inner()) = Some(authority.clone());
            let request = create(generation, "delivery-veto-create");
            armed.store(true, Ordering::SeqCst);
            let submission = product.exchange(request.raw()).unwrap();
            assert!(matches!(complete(&mut product, submission), Err(AgentError::Unknown)));
            assert_eq!(samples.load(Ordering::SeqCst), 4,
                "veto follows a real successful original-handle read");
            armed.store(false, Ordering::SeqCst);
            let delivery_time = if revoke_at_delivery { 6 } else { 31 };
            assert!(worker.native_command_queue().live_at(delivery_time),
                "main Control remains live; endpoint is the independent fence");
            assert!(!authority.is_live_at(delivery_time));
            assert_eq!(authority.is_closed(), revoke_at_delivery);
            assert!(product.status().delivery_unknown);
            assert!(matches!(product.poll(submission), Err(AgentError::Invalid)));
            let count = probe.enqueues.load(Ordering::SeqCst);
            assert!(matches!(product.exchange(request.raw()), Err(AgentError::Unknown)));
            assert_eq!(probe.enqueues.load(Ordering::SeqCst), count);
            callback_zero(&probe);
            drop(product);
            drop(reply);
            drop(authority);
            worker.stop();
            let exit = join(&mut worker);
            assert_eq!(exit.owner.runtime.binding(), binding);
            assert!(exit.disconnect.is_ok() && exit.maintenance.is_ok() && exit.cleanup.is_none());
        }
    }

    fn control_reply(product: &mut ProductSessionController, request: &Request) -> Reply {
        let submission = product.exchange(request.raw()).unwrap();
        let raw = match complete(product, submission).unwrap() {
            SessionCompletion::Reply(raw) => raw, _ => panic!("expected correlated control reply"),
        };
        Reply::decode_for(request, &raw).unwrap()
    }
    fn admitted(product: &mut ProductSessionController, sid: &str)
        -> crate::agent_tasks::agent_product::Writer {
        let submission = product.admit_writer(sid).unwrap();
        assert!(matches!(product.admit_writer(sid), Err(AgentError::Busy)));
        match complete(product, submission).unwrap() {
            SessionCompletion::WriterAdmitted { writer, generation } => {
                assert_eq!(Some(generation), product.status().endpoint_generation);
                assert!(matches!(product.poll(submission), Err(AgentError::Invalid)));
                writer
            }
            _ => panic!("expected genuine original writer admission"),
        }
    }
    fn writer_reply(product: &mut ProductSessionController,
        writer: crate::agent_tasks::agent_product::Writer, request: &Request) -> Reply {
        let submission = product.writer_exchange(writer, request.raw()).unwrap();
        let raw = match complete(product, submission).unwrap() {
            SessionCompletion::Reply(raw) => raw, _ => panic!("expected correlated writer reply"),
        };
        Reply::decode_for(request, &raw).unwrap()
    }
    fn writer_open(generation: u64, id: &str, sid: &str, epoch: u64) -> Request {
        Request::new_for_generation(id, generation,
            Action::OpenWriter { session_id: sid.into(), expected_epoch: epoch }).unwrap()
    }
    fn append(generation: u64, id: &str, sid: &str, epoch: u64, tail: u64, event: &str) -> Request {
        Request::new_for_generation(id, generation, Action::Append {
            session_id: sid.into(), epoch, expected_tail: tail,
            events: vec![morrow_agent_session_exec_v1_r2::Event {
                event_id: event.into(), body: b"ordinary product event".to_vec(),
            }],
        }).unwrap()
    }

    #[test]
    fn product_writer_create_admit_open_append_snapshot_close_keeps_control_and_original_owner() {
        let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        let (owner, package, context) = fixture_with_process(false);
        assert!(context.port().is_none());
        let binding = owner.runtime.binding();
        let mut worker = spawn(owner, package, context);
        let (mut product, probe) = controller(&worker, binding);
        let generation = opened(&mut product);
        assert!(matches!(control_reply(&mut product, &create(generation, "writer-create")).outcome,
            Outcome::Session(info) if info.epoch == 0 && info.tail == 0));
        let writer = admitted(&mut product, "session");
        assert!(matches!(product.admit_writer("session"), Err(AgentError::Invalid)));
        let open = writer_open(generation, "writer-open", "session", 0);
        assert!(matches!(product.exchange(open.raw()), Err(AgentError::Unsupported)));
        let epoch = match writer_reply(&mut product, writer, &open).outcome {
            Outcome::Session(info) => { assert_eq!(info.epoch, 1); info.epoch }
            _ => panic!("expected original writer CAS result"),
        };
        let request = append(generation, "writer-append", "session", epoch, 0, "event-one");
        assert!(matches!(writer_reply(&mut product, writer, &request).outcome,
            Outcome::Session(info) if info.epoch == epoch && info.tail == 1));
        let snapshot = Request::new_for_generation("writer-snapshot", generation,
            Action::Snapshot { session_id: "session".into(), after: 0, limit: 8 }).unwrap();
        assert!(matches!(writer_reply(&mut product, writer, &snapshot).outcome,
            Outcome::Snapshot(value) if value.info.tail == 1 && value.events.len() == 1
                && value.events[0].event.event_id == "event-one"
                && value.events[0].event.body == b"ordinary product event"));
        product.close_writer(writer).unwrap(); // Actual original admission.close.
        assert!(matches!(product.close_writer(writer), Err(AgentError::Invalid)));
        assert!(matches!(product.writer_exchange(writer, snapshot.raw()), Err(AgentError::Unavailable)));
        assert!(matches!(product.admit_writer("session"), Err(AgentError::Invalid)));
        assert!(product.status().original_lease_live && !product.status().delivery_unknown);
        let after = Request::new_for_generation("control-after-close", generation,
            Action::Snapshot { session_id: "session".into(), after: 0, limit: 8 }).unwrap();
        assert!(matches!(control_reply(&mut product, &after).outcome,
            Outcome::Snapshot(value) if value.info.tail == 1));
        assert!(matches!(product.retire("session"), Err(AgentError::Unsupported)));
        callback_zero(&probe);
        drop(product);
        worker.stop();
        let exit = join(&mut worker); // close_writer did not fabricate this join.
        assert_eq!(exit.owner.runtime.binding(), binding);
        assert!(exit.disconnect.is_ok() && exit.maintenance.is_ok() && exit.cleanup.is_none());
    }

    #[test]
    fn product_writer_exact_sid_generation_cas_and_duplicate_fences_preserve_real_tail() {
        let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        let (owner, package, context) = fixture_with_process(false);
        assert!(context.port().is_none());
        let binding = owner.runtime.binding();
        let mut worker = spawn(owner, package, context);
        let (mut product, probe) = controller(&worker, binding);
        let generation = opened(&mut product);
        control_reply(&mut product, &create(generation, "fenced-create"));
        let writer = admitted(&mut product, "session");
        let count = probe.enqueues.load(Ordering::SeqCst);
        let wrong_sid = writer_open(generation, "wrong-sid", "session-b", 0);
        assert!(matches!(product.writer_exchange(writer, wrong_sid.raw()),
            Err(AgentError::Session(morrow_agent_session_exec_v1_r2::Error::Denied))));
        let wrong_generation = writer_open(generation + 1, "wrong-writer-generation", "session", 0);
        assert!(matches!(product.writer_exchange(writer, wrong_generation.raw()),
            Err(AgentError::Session(morrow_agent_session_exec_v1_r2::Error::Correlation))));
        assert_eq!(probe.enqueues.load(Ordering::SeqCst), count);
        let open = writer_open(generation, "fenced-open", "session", 0);
        assert!(matches!(writer_reply(&mut product, writer, &open).outcome, Outcome::Session(info) if info.epoch == 1));
        let stale_open = writer_open(generation, "stale-open", "session", 0);
        assert!(matches!(writer_reply(&mut product, writer, &stale_open).outcome,
            Outcome::Rejected(morrow_agent_session_exec_v1_r2::Error::Conflict)));
        let wrong_tail = append(generation, "wrong-tail", "session", 1, 1, "event-one");
        assert!(matches!(writer_reply(&mut product, writer, &wrong_tail).outcome,
            Outcome::Rejected(morrow_agent_session_exec_v1_r2::Error::Conflict)));
        let request = append(generation, "accepted-append", "session", 1, 0, "event-one");
        assert!(matches!(writer_reply(&mut product, writer, &request).outcome,
            Outcome::Session(info) if info.tail == 1));
        let count = probe.enqueues.load(Ordering::SeqCst);
        assert!(matches!(product.writer_exchange(writer, request.raw()), Err(AgentError::Invalid)));
        assert_eq!(probe.enqueues.load(Ordering::SeqCst), count);
        let duplicate_event = append(generation, "duplicate-event", "session", 1, 1, "event-one");
        assert!(matches!(writer_reply(&mut product, writer, &duplicate_event).outcome,
            Outcome::Rejected(morrow_agent_session_exec_v1_r2::Error::Conflict)));
        let snapshot = Request::new_for_generation("fenced-snapshot", generation,
            Action::Snapshot { session_id: "session".into(), after: 0, limit: 8 }).unwrap();
        assert!(matches!(writer_reply(&mut product, writer, &snapshot).outcome,
            Outcome::Snapshot(value) if value.info.tail == 1 && value.events.len() == 1));
        assert!(!product.status().delivery_unknown);
        callback_zero(&probe);
        product.close_writer(writer).unwrap();
        drop(product); worker.stop();
        let exit = join(&mut worker); assert_eq!(exit.owner.runtime.binding(), binding);
    }

    #[test]
    fn product_writer_lost_append_delivery_seals_shared_lease_and_blocks_new_admission() {
        let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        let (owner, package, context) = fixture_with_process(false);
        assert!(context.port().is_none());
        let binding = owner.runtime.binding();
        let mut worker = spawn(owner, package, context);
        let (mut first, first_probe) = controller(&worker, binding);
        let generation = opened(&mut first);
        control_reply(&mut first, &create(generation, "lost-writer-create"));
        let writer = admitted(&mut first, "session");
        writer_reply(&mut first, writer, &writer_open(generation, "lost-writer-open", "session", 0));
        let request = append(generation, "lost-writer-append", "session", 1, 0, "lost-event");
        assert!(matches!(writer_reply(&mut first, writer, &request).outcome, Outcome::Session(info) if info.tail == 1));
        first.mark_delivery_unknown(); // Trusted outer delivery lost after real publication.
        let count = first_probe.enqueues.load(Ordering::SeqCst);
        assert!(matches!(first.writer_exchange(writer, request.raw()), Err(AgentError::Unknown)));
        assert!(matches!(first.admit_writer("session-b"), Err(AgentError::Unknown)));
        assert_eq!(first_probe.enqueues.load(Ordering::SeqCst), count);
        first.close_writer(writer).unwrap(); // Rights reduction still allowed while sealed.
        assert!(first.status().delivery_unknown);
        let (mut second, second_probe) = controller(&worker, binding);
        assert_eq!(opened(&mut second), generation);
        let snapshot = Request::new_for_generation("lost-writer-read", generation,
            Action::Snapshot { session_id: "session".into(), after: 0, limit: 8 }).unwrap();
        assert!(matches!(control_reply(&mut second, &snapshot).outcome,
            Outcome::Snapshot(value) if value.info.tail == 1 && value.events.len() == 1));
        let submission = second.admit_writer("session-b").unwrap();
        assert!(matches!(complete(&mut second, submission),
            Err(AgentError::Session(morrow_agent_session_exec_v1_r2::Error::CommitUnknown))));
        assert!(second.status().delivery_unknown);
        callback_zero(&first_probe); callback_zero(&second_probe);
        drop(second); drop(first); worker.stop();
        let exit = join(&mut worker); assert_eq!(exit.owner.runtime.binding(), binding);
    }

    #[test]
    fn product_foreign_writer_slot_cannot_exchange_or_revoke_and_pending_owned_revoke_stays_unknown() {
        let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        let (owner, package, context) = fixture_with_process(false);
        assert!(context.port().is_none());
        let binding = owner.runtime.binding();
        let mut worker = spawn(owner, package, context);
        let (mut first, first_probe) = controller(&worker, binding);
        let (mut second, second_probe) = controller(&worker, binding);
        let generation = opened(&mut first); assert_eq!(opened(&mut second), generation);
        control_reply(&mut first, &create(generation, "foreign-writer-create"));
        let first_writer = admitted(&mut first, "session");
        let second_writer = admitted(&mut second, "session-b");
        let open = writer_open(generation, "foreign-writer-open", "session", 0);
        let count = second_probe.enqueues.load(Ordering::SeqCst);
        assert!(matches!(second.writer_exchange(first_writer, open.raw()), Err(AgentError::Invalid)));
        assert!(matches!(second.close_writer(first_writer), Err(AgentError::Invalid)));
        assert!(matches!(first.close_writer(second_writer), Err(AgentError::Invalid)));
        assert_eq!(second_probe.enqueues.load(Ordering::SeqCst), count);
        assert!(matches!(writer_reply(&mut first, first_writer, &open).outcome, Outcome::Session(info) if info.epoch == 1));
        let request = append(generation, "revoked-pending-append", "session", 1, 0, "pending-event");
        let submission = first.writer_exchange(first_writer, request.raw()).unwrap();
        first.close_writer(first_writer).unwrap();
        assert_eq!(first.status().pending, Some(submission), "revocation discarded original handle");
        assert!(first.status().delivery_unknown);
        assert!(matches!(complete(&mut first, submission), Err(AgentError::Unknown | AgentError::Session(_))));
        assert!(matches!(first.poll(submission), Err(AgentError::Invalid)));
        assert!(matches!(first.writer_exchange(first_writer, request.raw()), Err(AgentError::Unknown)));
        second.close_writer(second_writer).unwrap();
        assert!(second.status().original_lease_live, "writer revocation closed control endpoint");
        callback_zero(&first_probe); callback_zero(&second_probe);
        drop(second); drop(first); worker.stop();
        let exit = join(&mut worker); assert_eq!(exit.owner.runtime.binding(), binding);
    }

    #[test]
    fn product_writer_admission_consumes_original_worker_command_budget() {
        let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        let (owner, package, context) = fixture_with_process(false);
        assert!(context.port().is_none());
        let binding = owner.runtime.binding();
        let mut worker = AgentWorker::spawn(owner, package, context, ProductGate::default(),
            AgentLimits { max_commands: 4, ..AgentLimits::default() }, 60_000, Arc::new(|| 6))
            .unwrap_or_else(|_| panic!("ordinary owner spawn failed"));
        let (mut product, probe) = controller(&worker, binding);
        let generation = opened(&mut product); // original command 1
        control_reply(&mut product, &create(generation, "budget-create")); // 2
        let writer = admitted(&mut product, "session"); // 3, not a free admission
        let list = Request::new_for_generation("budget-list", generation, Action::List).unwrap();
        assert!(matches!(control_reply(&mut product, &list).outcome, Outcome::Sessions(_))); // 4
        assert!(matches!(product.admit_writer("session-b"), Err(AgentError::Limit)));
        assert!(!product.status().delivery_unknown, "known prequeue Limit invented Unknown");
        product.close_writer(writer).unwrap(); // no extra queue command, no fake release/join
        callback_zero(&probe);
        drop(product); worker.stop();
        let exit = join(&mut worker); assert_eq!(exit.owner.runtime.binding(), binding);
    }
}
