#![cfg(all(feature = "host", not(target_arch = "wasm32")))]
use morrow_agent_process_control_v1::{host::*, *};
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};
struct Provider {
    calls: Arc<AtomicUsize>,
    outcome: EffectOutcome,
    panic: bool,
    fail_read: bool,
    exited: bool,
}
fn supported() -> Capabilities {
    Capabilities {
        read: true,
        events: true,
        write: true,
        close_input: true,
        interrupt: true,
        terminate: true,
        resize_pty: false,
    }
}
impl ProcessProvider for Provider {
    fn capabilities(&self) -> Capabilities {
        supported()
    }
    fn read(&mut self, q: ReadQuery) -> Result<OutputPage> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        if self.fail_read {
            return Err(Error::Unknown);
        }
        Ok(OutputPage {
            events: Vec::new(),
            next_seq: q.after_seq,
            floor_seq: 1,
            gap: false,
            exited: self.exited,
            exit_code: self.exited.then_some(0),
            closed: false,
            failure: None,
        })
    }
    fn events(&mut self, q: ReadQuery) -> Result<OutputPage> {
        self.read(q)
    }
    fn write(&mut self, _: &[u8]) -> EffectOutcome {
        self.calls.fetch_add(1, Ordering::SeqCst);
        assert!(!self.panic, "injected provider panic");
        self.outcome
    }
    fn close_input(&mut self) -> EffectOutcome {
        self.write(&[])
    }
    fn interrupt(&mut self) -> EffectOutcome {
        self.write(&[])
    }
    fn terminate(&mut self) -> EffectOutcome {
        self.write(&[])
    }
    fn resize(&mut self, _: u16, _: u16) -> EffectOutcome {
        panic!("unsupported must never call provider")
    }
}
fn binding() -> Binding {
    Binding {
        connection: [1; 32],
        session_id: "session".into(),
        operation_id: "operation".into(),
        generation: 1,
        created_at_ms: 1,
        expires_at_ms: 100,
    }
}
fn fixture(outcome: EffectOutcome, budget: Budget) -> (Host, Handle, Arc<AtomicUsize>) {
    let mut host = Host::default();
    let calls = Arc::new(AtomicUsize::new(0));
    let handle = host
        .register(
            binding(),
            [2; 32],
            supported(),
            budget,
            Box::new(Provider {
                calls: calls.clone(),
                outcome,
                panic: false,
                fail_read: false,
                exited: false,
            }),
        )
        .unwrap();
    (host, handle, calls)
}
fn req(handle: Handle, id: &str, action: Action) -> Request {
    Request::new(id, handle.nonce, handle.generation, action).unwrap()
}
fn dispatch(host: &mut Host, request: &Request) -> Result<ReplyBody> {
    host.dispatch([1; 32], request, || 1, |_| true)
}
fn read_action() -> Action {
    Action::Read(ReadQuery {
        after_seq: 0,
        max_bytes: 10,
        max_events: 16,
        wait_ms: 0,
    })
}
#[test]
fn connection_generation_expiry_revocation_and_live_authorization_fail_closed() {
    let (mut host, h, c) = fixture(EffectOutcome::Accepted, Budget::default());
    let r = req(h, "x", Action::Write(vec![1]));
    assert_eq!(
        host.dispatch([9; 32], &r, || 1, |_| true),
        Err(Error::Denied)
    );
    let mut g = r.clone();
    g.generation = 2;
    assert_eq!(dispatch(&mut host, &g), Err(Error::Denied));
    assert_eq!(
        host.dispatch([1; 32], &r, || 100, |_| true),
        Err(Error::Denied)
    );
    assert_eq!(
        host.dispatch([1; 32], &r, || 1, |_| false),
        Err(Error::Denied)
    );
    assert_eq!(c.load(Ordering::SeqCst), 0);
    host.revoke(h).unwrap();
    assert_eq!(
        dispatch(&mut host, &req(h, "read", read_action())),
        Err(Error::Denied)
    );
    assert_eq!(host.trusted_cleanup(h).unwrap(), EffectOutcome::Accepted);
    assert_eq!(c.load(Ordering::SeqCst), 1);
}
#[test]
fn capabilities_intersect_and_unsupported_never_dispatches() {
    let (mut host, h, c) = fixture(EffectOutcome::Accepted, Budget::default());
    assert_eq!(
        dispatch(
            &mut host,
            &req(h, "resize", Action::Resize { rows: 24, cols: 80 })
        ),
        Err(Error::Unsupported)
    );
    assert_eq!(c.load(Ordering::SeqCst), 0);
    assert_eq!(
        dispatch(&mut host, &req(h, "caps", Action::Discover)),
        Ok(ReplyBody::Capabilities(supported()))
    )
}
#[test]
fn accepted_control_receipt_deduplicates_and_mismatched_id_conflicts() {
    let (mut host, h, c) = fixture(EffectOutcome::Accepted, Budget::default());
    let r = req(h, "write", Action::Write(vec![1]));
    assert_eq!(dispatch(&mut host, &r), Ok(ReplyBody::Accepted));
    assert_eq!(dispatch(&mut host, &r), Ok(ReplyBody::Accepted));
    assert_eq!(
        dispatch(&mut host, &req(h, "write", Action::Write(vec![2]))),
        Err(Error::Conflict)
    );
    assert_eq!(c.load(Ordering::SeqCst), 1)
}
#[test]
fn unknown_receipt_never_replays_and_freezes_controls_but_allows_read() {
    let (mut host, h, c) = fixture(EffectOutcome::Unknown, Budget::default());
    let r = req(h, "write", Action::Write(vec![1]));
    assert_eq!(dispatch(&mut host, &r), Err(Error::Unknown));
    assert_eq!(dispatch(&mut host, &r), Err(Error::Unknown));
    assert_eq!(
        dispatch(&mut host, &req(h, "other", Action::Terminate)),
        Err(Error::Unknown)
    );
    assert!(dispatch(&mut host, &req(h, "read", read_action())).is_ok());
    assert_eq!(c.load(Ordering::SeqCst), 2)
}
#[test]
fn post_dispatch_expiry_leaves_unknown_and_cannot_return_accepted() {
    let (mut host, h, c) = fixture(EffectOutcome::Accepted, Budget::default());
    let mut clock = vec![100, 1];
    let r = req(h, "write", Action::Write(vec![1]));
    assert_eq!(
        host.dispatch([1; 32], &r, || clock.pop().unwrap(), |_| true),
        Err(Error::Unknown)
    );
    assert_eq!(dispatch(&mut host, &r), Err(Error::Denied));
    assert!(host.veto_delivery(&r).is_ok());
    assert_eq!(c.load(Ordering::SeqCst), 1)
}
#[test]
fn close_input_and_observed_exit_reject_later_controls() {
    let (mut host, h, c) = fixture(EffectOutcome::Accepted, Budget::default());
    assert_eq!(
        dispatch(&mut host, &req(h, "close", Action::CloseInput)),
        Ok(ReplyBody::Accepted)
    );
    assert_eq!(
        dispatch(&mut host, &req(h, "write", Action::Write(vec![1]))),
        Err(Error::Closed)
    );
    assert_eq!(
        dispatch(&mut host, &req(h, "term", Action::Terminate)),
        Ok(ReplyBody::Accepted)
    );
    assert_eq!(c.load(Ordering::SeqCst), 2)
}
#[test]
fn consumed_budgets_never_refund_rejected_or_failed_provider_calls() {
    let (mut host, h, c) = fixture(
        EffectOutcome::Rejected(Error::Closed),
        Budget {
            max_input_bytes: 1,
            max_control_calls: 1,
            max_read_calls: 1,
            max_output_bytes: 10,
        },
    );
    assert_eq!(
        dispatch(&mut host, &req(h, "w1", Action::Write(vec![1]))),
        Err(Error::Closed)
    );
    assert_eq!(
        dispatch(&mut host, &req(h, "w2", Action::Write(vec![1]))),
        Err(Error::Limit)
    );
    assert!(dispatch(&mut host, &req(h, "r1", read_action())).is_ok());
    assert_eq!(
        dispatch(&mut host, &req(h, "r2", read_action())),
        Err(Error::Limit)
    );
    assert_eq!(c.load(Ordering::SeqCst), 2)
}
#[test]
fn provider_panic_preserves_unknown_reservation() {
    let mut host = Host::default();
    let c = Arc::new(AtomicUsize::new(0));
    let h = host
        .register(
            binding(),
            [2; 32],
            supported(),
            Budget::default(),
            Box::new(Provider {
                calls: c.clone(),
                outcome: EffectOutcome::Accepted,
                panic: true,
                fail_read: false,
                exited: false,
            }),
        )
        .unwrap();
    let r = req(h, "write", Action::Write(vec![1]));
    assert!(
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| dispatch(&mut host, &r))).is_err()
    );
    assert_eq!(dispatch(&mut host, &r), Err(Error::Unknown));
    assert_eq!(
        dispatch(&mut host, &req(h, "later", Action::Terminate)),
        Err(Error::Unknown)
    );
    assert_eq!(c.load(Ordering::SeqCst), 1)
}
#[test]
fn registration_limits_duplicate_binding_and_restart_handle_invalidation() {
    let (mut host, h, _) = fixture(EffectOutcome::Accepted, Budget::default());
    let provider = || {
        Box::new(Provider {
            calls: Arc::new(AtomicUsize::new(0)),
            outcome: EffectOutcome::Accepted,
            panic: false,
            fail_read: false,
            exited: false,
        }) as Box<dyn ProcessProvider>
    };
    assert_eq!(
        host.register(
            binding(),
            [3; 32],
            supported(),
            Budget::default(),
            provider()
        ),
        Err(Error::Conflict)
    );
    for n in 3..18 {
        let mut b = binding();
        b.operation_id = format!("op-{n}");
        assert!(
            host.register(b, [n; 32], supported(), Budget::default(), provider())
                .is_ok()
        )
    }
    let mut b = binding();
    b.operation_id = "overflow".into();
    assert_eq!(
        host.register(b, [99; 32], supported(), Budget::default(), provider()),
        Err(Error::Limit)
    );
    let mut restarted = Host::default();
    assert_eq!(
        dispatch(&mut restarted, &req(h, "stale", read_action())),
        Err(Error::NotFound)
    )
}
#[test]
fn backward_clock_permanently_revokes_and_discovery_rechecks_delivery() {
    let (mut host, h, c) = fixture(EffectOutcome::Accepted, Budget::default());
    let r = req(h, "caps", Action::Discover);
    assert!(host.dispatch([1; 32], &r, || 20, |_| true).is_ok());
    assert_eq!(
        host.dispatch([1; 32], &r, || 19, |_| true),
        Err(Error::Denied)
    );
    assert_eq!(
        host.dispatch([1; 32], &r, || 21, |_| true),
        Err(Error::Denied)
    );
    assert_eq!(c.load(Ordering::SeqCst), 0);
    let (mut host, h, _) = fixture(EffectOutcome::Accepted, Budget::default());
    let mut auth = vec![false, true];
    assert_eq!(
        host.dispatch(
            [1; 32],
            &req(h, "caps", Action::Discover),
            || 1,
            |_| auth.pop().unwrap()
        ),
        Err(Error::Denied)
    );
}
#[test]
fn duplicate_receipt_checks_delivery_and_veto_marks_only_exact_accepted_effect() {
    let (mut host, h, c) = fixture(EffectOutcome::Accepted, Budget::default());
    let r = req(h, "write", Action::Write(vec![1]));
    assert!(dispatch(&mut host, &r).is_ok());
    let mut mismatch = r.clone();
    mismatch.action = Action::Write(vec![2]);
    assert_eq!(host.veto_delivery(&mismatch), Err(Error::Conflict));
    assert_eq!(
        host.veto_delivery(&req(h, "read", read_action())),
        Err(Error::Invalid)
    );
    host.veto_delivery(&r).unwrap();
    assert_eq!(dispatch(&mut host, &r), Err(Error::Unknown));
    assert_eq!(
        dispatch(&mut host, &req(h, "term", Action::Terminate)),
        Err(Error::Unknown)
    );
    assert_eq!(c.load(Ordering::SeqCst), 1);
    let (mut host, h, c) = fixture(EffectOutcome::Accepted, Budget::default());
    let r = req(h, "w", Action::Write(vec![1]));
    assert!(dispatch(&mut host, &r).is_ok());
    let mut auth = vec![false, true];
    assert_eq!(
        host.dispatch([1; 32], &r, || 1, |_| auth.pop().unwrap()),
        Err(Error::Denied)
    );
    assert_eq!(c.load(Ordering::SeqCst), 1);
    let (mut host, h, _) = fixture(EffectOutcome::Rejected(Error::Closed), Budget::default());
    let r = req(h, "w", Action::Write(vec![1]));
    assert_eq!(dispatch(&mut host, &r), Err(Error::Closed));
    assert_eq!(host.veto_delivery(&r), Err(Error::Conflict));
}
struct Lifecycle {
    pages: std::collections::VecDeque<OutputPage>,
    drops: Arc<AtomicUsize>,
}
impl Drop for Lifecycle {
    fn drop(&mut self) {
        self.drops.fetch_add(1, Ordering::SeqCst);
    }
}
impl ProcessProvider for Lifecycle {
    fn capabilities(&self) -> Capabilities {
        supported()
    }
    fn read(&mut self, _: ReadQuery) -> Result<OutputPage> {
        self.pages.pop_front().ok_or(Error::Unknown)
    }
    fn events(&mut self, q: ReadQuery) -> Result<OutputPage> {
        self.read(q)
    }
    fn write(&mut self, _: &[u8]) -> EffectOutcome {
        EffectOutcome::Accepted
    }
    fn close_input(&mut self) -> EffectOutcome {
        EffectOutcome::Accepted
    }
    fn interrupt(&mut self) -> EffectOutcome {
        EffectOutcome::Accepted
    }
    fn terminate(&mut self) -> EffectOutcome {
        EffectOutcome::Accepted
    }
    fn resize(&mut self, _: u16, _: u16) -> EffectOutcome {
        EffectOutcome::Rejected(Error::Unsupported)
    }
}
fn status(exited: bool, code: Option<i32>, closed: bool) -> OutputPage {
    OutputPage {
        events: vec![],
        next_seq: 0,
        floor_seq: 1,
        gap: false,
        exited,
        exit_code: code,
        closed,
        failure: None,
    }
}
#[test]
fn exited_is_separate_from_eof_and_finish_drops_before_releasing_slot() {
    let mut host = Host::default();
    let drops = Arc::new(AtomicUsize::new(0));
    let h = host
        .register(
            binding(),
            [2; 32],
            supported(),
            Budget::default(),
            Box::new(Lifecycle {
                pages: vec![status(true, Some(0), false), status(true, Some(0), true)].into(),
                drops: drops.clone(),
            }),
        )
        .unwrap();
    assert_eq!(host.finish(h), Err(Error::Conflict));
    assert!(dispatch(&mut host, &req(h, "exit", read_action())).is_ok());
    assert_eq!(host.finish(h), Err(Error::Conflict));
    assert_eq!(
        dispatch(&mut host, &req(h, "w", Action::Write(vec![1]))),
        Err(Error::Closed)
    );
    assert_eq!(drops.load(Ordering::SeqCst), 0);
    host.revoke(h).unwrap();
    assert!(
        host.trusted_observe(
            h,
            ReadQuery {
                after_seq: 0,
                max_bytes: 10,
                max_events: 16,
                wait_ms: 0
            }
        )
        .unwrap()
        .closed
    );
    host.finish(h).unwrap();
    assert_eq!(drops.load(Ordering::SeqCst), 1);
    assert_eq!(host.binding(h), Err(Error::NotFound));
    let mut b = binding();
    b.operation_id = "new-op".into();
    assert_eq!(
        host.register(
            b,
            [2; 32],
            supported(),
            Budget::default(),
            Box::new(Lifecycle {
                pages: vec![].into(),
                drops: drops.clone()
            })
        ),
        Err(Error::Conflict)
    );
}
#[test]
fn observed_lifecycle_and_exit_code_cannot_regress() {
    for bad in [
        status(false, None, false),
        status(true, Some(2), true),
        status(true, Some(1), false),
        status(true, None, true),
    ] {
        let mut host = Host::default();
        let h = host
            .register(
                binding(),
                [2; 32],
                supported(),
                Budget::default(),
                Box::new(Lifecycle {
                    pages: vec![status(true, Some(1), true), bad].into(),
                    drops: Arc::new(AtomicUsize::new(0)),
                }),
            )
            .unwrap();
        assert!(dispatch(&mut host, &req(h, "r1", read_action())).is_ok());
        assert_eq!(
            dispatch(&mut host, &req(h, "r2", read_action())),
            Err(Error::Invalid)
        );
    }
}
#[test]
fn failed_read_consumes_reserved_budget_and_oversized_registration_budget_rejects() {
    let mut host = Host::default();
    let c = Arc::new(AtomicUsize::new(0));
    let provider = || {
        Box::new(Provider {
            calls: c.clone(),
            outcome: EffectOutcome::Accepted,
            panic: false,
            fail_read: true,
            exited: false,
        }) as Box<dyn ProcessProvider>
    };
    assert_eq!(
        host.register(
            binding(),
            [2; 32],
            supported(),
            Budget {
                max_read_calls: 129,
                ..Budget::default()
            },
            provider()
        ),
        Err(Error::Limit)
    );
    let h = host
        .register(
            binding(),
            [2; 32],
            supported(),
            Budget {
                max_read_calls: 1,
                max_output_bytes: 10,
                ..Budget::default()
            },
            provider(),
        )
        .unwrap();
    assert_eq!(
        dispatch(&mut host, &req(h, "r1", read_action())),
        Err(Error::Unknown)
    );
    assert_eq!(
        dispatch(&mut host, &req(h, "r2", read_action())),
        Err(Error::Limit)
    );
    assert_eq!(c.load(Ordering::SeqCst), 1);
}
#[test]
fn host_approval_restricts_real_provider_support() {
    let mut host = Host::default();
    let calls = Arc::new(AtomicUsize::new(0));
    let approved = Capabilities {
        read: true,
        ..Capabilities::default()
    };
    let h = host
        .register(
            binding(),
            [2; 32],
            approved,
            Budget::default(),
            Box::new(Provider {
                calls: calls.clone(),
                outcome: EffectOutcome::Accepted,
                panic: false,
                fail_read: false,
                exited: false,
            }),
        )
        .unwrap();
    assert_eq!(
        dispatch(&mut host, &req(h, "caps", Action::Discover)),
        Ok(ReplyBody::Capabilities(approved))
    );
    assert_eq!(
        dispatch(&mut host, &req(h, "write", Action::Write(vec![1]))),
        Err(Error::Unsupported)
    );
    assert_eq!(calls.load(Ordering::SeqCst), 0);
}
#[test]
fn finished_process_slots_are_reusable_but_tombstones_never_evicted() {
    let mut host = Host::default();
    let drops = Arc::new(AtomicUsize::new(0));
    for n in 1..=MAX_TOMBSTONES {
        let mut b = binding();
        b.operation_id = format!("op-{n}");
        let mut nonce = [0; 32];
        nonce[..8].copy_from_slice(&(n as u64).to_le_bytes());
        let h = host
            .register(
                b,
                nonce,
                supported(),
                Budget::default(),
                Box::new(Lifecycle {
                    pages: vec![status(true, Some(0), true)].into(),
                    drops: drops.clone(),
                }),
            )
            .unwrap();
        host.trusted_observe(
            h,
            ReadQuery {
                after_seq: 0,
                max_bytes: 1,
                max_events: 1,
                wait_ms: 0,
            },
        )
        .unwrap();
        host.finish(h).unwrap();
    }
    assert_eq!(drops.load(Ordering::SeqCst), MAX_TOMBSTONES);
    let mut b = binding();
    b.operation_id = "overflow".into();
    assert_eq!(
        host.register(
            b,
            [9; 32],
            supported(),
            Budget::default(),
            Box::new(Lifecycle {
                pages: vec![].into(),
                drops: drops.clone()
            })
        ),
        Err(Error::Limit)
    );
}
struct PanicDrop(Lifecycle);
impl Drop for PanicDrop {
    fn drop(&mut self) {
        panic!("injected teardown panic")
    }
}
impl ProcessProvider for PanicDrop {
    fn capabilities(&self) -> Capabilities {
        supported()
    }
    fn read(&mut self, q: ReadQuery) -> Result<OutputPage> {
        self.0.read(q)
    }
    fn events(&mut self, q: ReadQuery) -> Result<OutputPage> {
        self.0.events(q)
    }
    fn write(&mut self, c: &[u8]) -> EffectOutcome {
        self.0.write(c)
    }
    fn close_input(&mut self) -> EffectOutcome {
        self.0.close_input()
    }
    fn interrupt(&mut self) -> EffectOutcome {
        self.0.interrupt()
    }
    fn terminate(&mut self) -> EffectOutcome {
        self.0.terminate()
    }
    fn resize(&mut self, r: u16, c: u16) -> EffectOutcome {
        self.0.resize(r, c)
    }
}
#[test]
fn teardown_panic_cannot_free_active_slot_or_reuse_nonce() {
    let mut host = Host::default();
    let h = host
        .register(
            binding(),
            [2; 32],
            supported(),
            Budget::default(),
            Box::new(PanicDrop(Lifecycle {
                pages: vec![status(true, Some(0), true)].into(),
                drops: Arc::new(AtomicUsize::new(0)),
            })),
        )
        .unwrap();
    host.trusted_observe(
        h,
        ReadQuery {
            after_seq: 0,
            max_bytes: 1,
            max_events: 1,
            wait_ms: 0,
        },
    )
    .unwrap();
    assert!(std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| host.finish(h))).is_err());
    assert_eq!(host.finish(h), Err(Error::Unknown));
    assert_eq!(
        dispatch(&mut host, &req(h, "caps", Action::Discover)),
        Err(Error::Denied)
    );
    assert_eq!(host.binding(h).unwrap(), binding());
    let mut b = binding();
    b.operation_id = "new".into();
    assert_eq!(
        host.register(
            b,
            [2; 32],
            supported(),
            Budget::default(),
            Box::new(Lifecycle {
                pages: vec![].into(),
                drops: Arc::new(AtomicUsize::new(0))
            })
        ),
        Err(Error::Conflict)
    );
}
#[test]
fn authorization_observes_each_fresh_clock_sample() {
    let (mut host, h, _) = fixture(EffectOutcome::Accepted, Budget::default());
    let sample = std::cell::Cell::new(10u64);
    let seen = std::cell::Cell::new(10u64);
    for action in [
        Action::Discover,
        Action::Write(vec![1]),
        read_action(),
        Action::Write(vec![1]),
    ] {
        let id = if matches!(action, Action::Write(_)) {
            "write"
        } else {
            "read"
        };
        assert!(
            host.dispatch(
                [1; 32],
                &req(h, id, action),
                || {
                    let now = sample.get() + 1;
                    sample.set(now);
                    now
                },
                |_| {
                    assert!(
                        sample.get() > seen.get(),
                        "authorization used an old clock sample"
                    );
                    seen.set(sample.get());
                    true
                }
            )
            .is_ok()
        );
    }
}
