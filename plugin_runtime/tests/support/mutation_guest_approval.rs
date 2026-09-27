//! Opt-in guest approval still runs on the original Windows owner and target.
//! All native effects in this module are confined to Fixture's temporary tree.
use super::*;

fn digest(plan: &RequestRecord) -> [u8; 32] {
    Sha256::digest(plan.container()).into()
}

fn approved(handle: &mut MutationHandle) -> morrow_plugin_runtime::io_jobs::MutationGuestLease {
    let MutationResponse::GuestApproved(lease) = read(handle).unwrap() else {
        panic!("expected delivered guest approval");
    };
    lease
}

fn permitted(
    handle: &mut MutationHandle,
) -> morrow_plugin_runtime::io_jobs::MutationGuestExecutionPermit {
    let MutationResponse::GuestExecutionAuthorized(permit) = read(handle).unwrap() else {
        panic!("expected delivered execution permit");
    };
    permit
}

#[test]
fn create_requires_separate_delivered_permit_and_consumes_it_once() {
    let _serial = serial_effects();
    let (dir, _, mut worker) = Fixture::new().start();
    let root = dir.path().join("guest-create");
    fs::create_dir(&root).unwrap();
    let leaf = root.join("created.bin");
    let body = b"reviewed guest content";
    let (session, mut selection) = worker
        .select_mutation_create(
            root,
            RelativeFilePath::parse("created.bin").unwrap(),
            scope(Disposition::Create),
            [70; 32],
        )
        .unwrap();
    selected(&mut selection);
    let plan = planned(
        &mut worker
            .build_mutation_plan(
                session,
                "guest-create-once".into(),
                body.len() as u64,
                Some(Sha256::digest(body).into()),
            )
            .unwrap(),
    );
    let lease = approved(
        &mut worker
            .issue_mutation_guest(session, plan.clone(), [71; 32])
            .unwrap(),
    );
    assert_eq!(lease.reference(), [71; 32]);
    assert_eq!(lease.plan_sha256(), digest(&plan));
    assert_eq!(
        read(
            &mut worker
                .authorize_mutation_guest_execution(session, digest(&plan))
                .unwrap()
        )
        .err(),
        Some(TargetError::Missing)
    ); // no durable Prepare record yet
    assert!(matches!(
        read(&mut worker.prepare_mutation(session, plan.clone()).unwrap()),
        Ok(MutationResponse::Prepared(_))
    ));
    assert_eq!(
        read(
            &mut worker
                .authorize_mutation_guest_execution(session, digest(&plan))
                .unwrap()
        )
        .err(),
        Some(TargetError::Missing)
    ); // prepared, but no committed content
    assert!(matches!(
        read(&mut worker.stage_mutation_chunk(session, 0, body.to_vec()).unwrap()),
        Ok(MutationResponse::Staged { bytes, durable: false }) if bytes == body.len() as u64
    ));
    assert_eq!(
        read(
            &mut worker
                .authorize_mutation_guest_execution(session, digest(&plan))
                .unwrap()
        )
        .err(),
        Some(TargetError::Missing)
    ); // resident spool is not durable content
    assert!(matches!(
        read(&mut worker.commit_mutation_content(session).unwrap()),
        Ok(MutationResponse::Staged { bytes, durable: true }) if bytes == body.len() as u64
    ));
    assert_eq!(
        read(&mut worker.execute_mutation(session).unwrap()).err(),
        Some(TargetError::Busy)
    );
    assert!(!leaf.exists());
    assert_eq!(
        read(
            &mut worker
                .authorize_mutation_guest_execution(session, [8; 32])
                .unwrap()
        )
        .err(),
        Some(TargetError::Mismatch)
    );
    let permit = permitted(
        &mut worker
            .authorize_mutation_guest_execution(session, digest(&plan))
            .unwrap(),
    );
    assert_eq!(permit.lease(), lease);
    assert!(matches!(
        read(&mut worker.execute_mutation(session).unwrap()),
        Ok(MutationResponse::Created(_))
    ));
    assert_eq!(fs::read(&leaf).unwrap(), body);
    assert_eq!(
        read(&mut worker.execute_mutation(session).unwrap()).err(),
        Some(TargetError::AlreadyDispatched)
    );
    assert_eq!(
        read(
            &mut worker
                .authorize_mutation_guest_execution(session, digest(&plan))
                .unwrap()
        )
        .err(),
        Some(TargetError::AlreadyDispatched)
    );
    assert!(matches!(
        read(&mut worker.release_mutation(session).unwrap()),
        Ok(MutationResponse::Released)
    ));
    assert_eq!(reclaim(&mut worker).disconnect, Ok(()));
}

#[test]
fn delete_runs_only_after_explicit_permit_and_cancelled_plan_never_runs() {
    let _serial = serial_effects();
    let (dir, _, mut worker) = Fixture::new().start();
    for (index, cancel) in [false, true].into_iter().enumerate() {
        let leaf = dir.path().join(format!("guest-delete-{index}.bin"));
        fs::write(&leaf, b"reviewed original").unwrap();
        let (session, mut selection) = worker
            .select_mutation_existing(
                leaf.clone(),
                scope(Disposition::Delete),
                [81 + index as u8; 32],
            )
            .unwrap();
        selected(&mut selection);
        let plan = planned(
            &mut worker
                .build_mutation_plan(session, format!("guest-delete-{index}"), 0, None)
                .unwrap(),
        );
        approved(
            &mut worker
                .issue_mutation_guest(session, plan.clone(), [83 + index as u8; 32])
                .unwrap(),
        );
        assert!(matches!(
            read(&mut worker.prepare_mutation(session, plan.clone()).unwrap()),
            Ok(MutationResponse::Prepared(_))
        ));
        if cancel {
            assert!(matches!(
                read(&mut worker.cancel_mutation_plan(session).unwrap()),
                Ok(MutationResponse::PlanCancelled(_))
            ));
            assert_eq!(
                read(
                    &mut worker
                        .authorize_mutation_guest_execution(session, digest(&plan))
                        .unwrap()
                )
                .err(),
                Some(TargetError::Missing)
            );
            assert!(read(&mut worker.execute_mutation(session).unwrap()).is_err());
        } else {
            permitted(
                &mut worker
                    .authorize_mutation_guest_execution(session, digest(&plan))
                    .unwrap(),
            );
            assert!(matches!(
                read(&mut worker.execute_mutation(session).unwrap()),
                Ok(MutationResponse::Deleted(_))
            ));
        }
        assert!(matches!(
            read(&mut worker.release_mutation(session).unwrap()),
            Ok(MutationResponse::Released)
        ));
        if cancel {
            assert_eq!(fs::read(&leaf).unwrap(), b"reviewed original");
        } else {
            assert!(!leaf.exists());
        }
    }
    assert_eq!(reclaim(&mut worker).disconnect, Ok(()));
}

#[test]
fn delete_lost_permit_receipt_cannot_execute_or_reauthorize() {
    let _serial = serial_effects();
    let (dir, _, mut worker) = Fixture::new().start();
    let leaf = dir.path().join("guest-delete-lost.bin");
    fs::write(&leaf, b"must remain").unwrap();
    let (session, mut selection) = worker
        .select_mutation_existing(leaf.clone(), scope(Disposition::Delete), [72; 32])
        .unwrap();
    selected(&mut selection);
    let plan = planned(
        &mut worker
            .build_mutation_plan(session, "guest-delete-lost".into(), 0, None)
            .unwrap(),
    );
    approved(
        &mut worker
            .issue_mutation_guest(session, plan.clone(), [73; 32])
            .unwrap(),
    );
    assert!(matches!(
        read(&mut worker.prepare_mutation(session, plan.clone()).unwrap()),
        Ok(MutationResponse::Prepared(_))
    ));
    let mut permit = worker
        .authorize_mutation_guest_execution(session, digest(&plan))
        .unwrap();
    ready(&permit);
    assert!(permit.is_started());
    assert_eq!(
        read(&mut worker.execute_mutation(session).unwrap()).err(),
        Some(TargetError::Busy)
    );
    permit.cancel();
    assert_eq!(permit.read().err(), Some(OwnerCommandError::Unknown));
    assert_eq!(
        read(&mut worker.execute_mutation(session).unwrap()).err(),
        Some(TargetError::Busy)
    );
    assert_eq!(
        read(
            &mut worker
                .authorize_mutation_guest_execution(session, digest(&plan))
                .unwrap()
        )
        .err(),
        Some(TargetError::AlreadyDispatched)
    );
    assert!(matches!(
        read(&mut worker.release_mutation(session).unwrap()),
        Ok(MutationResponse::Released)
    ));
    assert_eq!(fs::read(&leaf).unwrap(), b"must remain");
    assert_eq!(reclaim(&mut worker).disconnect, Ok(()));
}

#[test]
fn approval_requires_delivered_selection_and_cannot_switch_plan_or_reuse_reference() {
    let _serial = serial_effects();
    let (dir, package, mut worker) = Fixture::new().start();
    let root = dir.path().join("guest-approval");
    fs::create_dir(&root).unwrap();
    let (session, mut selection) = worker
        .select_mutation_create(
            root.clone(),
            RelativeFilePath::parse("first.bin").unwrap(),
            scope(Disposition::Create),
            [74; 32],
        )
        .unwrap();
    ready(&selection); // the owner ran selection, but the caller did not read it
    let plan = planned(
        &mut worker
            .build_mutation_plan(
                session,
                "guest-first".into(),
                0,
                Some(Sha256::digest([]).into()),
            )
            .unwrap(),
    );
    assert_eq!(
        read(
            &mut worker
                .issue_mutation_guest(session, plan.clone(), [75; 32])
                .unwrap()
        )
        .err(),
        Some(TargetError::Busy)
    );
    selected(&mut selection);
    let mut forged = plan.request().clone();
    forged.subject = "other-subject".into();
    assert_eq!(
        read(
            &mut worker
                .issue_mutation_guest(session, RequestRecord::new(forged).unwrap(), [75; 32])
                .unwrap()
        )
        .err(),
        Some(TargetError::Mismatch)
    );
    approved(
        &mut worker
            .issue_mutation_guest(session, plan.clone(), [75; 32])
            .unwrap(),
    );
    let switched = request(
        package,
        "guest-other-plan",
        Disposition::Create,
        plan.request().target.reference,
        plan.request().target.relative_path.clone(),
        None,
        b"",
    );
    assert_eq!(
        read(&mut worker.prepare_mutation(session, switched).unwrap()).err(),
        Some(TargetError::Mismatch)
    );
    assert!(matches!(
        read(&mut worker.release_mutation(session).unwrap()),
        Ok(MutationResponse::Released)
    ));
    let (other, mut selection) = worker
        .select_mutation_create(
            root,
            RelativeFilePath::parse("second.bin").unwrap(),
            scope(Disposition::Create),
            [76; 32],
        )
        .unwrap();
    selected(&mut selection);
    let other_plan = planned(
        &mut worker
            .build_mutation_plan(
                other,
                "guest-second".into(),
                0,
                Some(Sha256::digest([]).into()),
            )
            .unwrap(),
    );
    assert_eq!(
        read(
            &mut worker
                .issue_mutation_guest(other, other_plan.clone(), [75; 32])
                .unwrap()
        )
        .err(),
        Some(TargetError::Limit)
    );
    approved(
        &mut worker
            .issue_mutation_guest(other, other_plan, [77; 32])
            .unwrap(),
    );
    assert!(matches!(
        read(&mut worker.release_mutation(other).unwrap()),
        Ok(MutationResponse::Released)
    ));
    assert_eq!(reclaim(&mut worker).disconnect, Ok(()));
}

#[test]
fn lost_issue_receipt_stays_opted_in_and_blocks_effect() {
    let _serial = serial_effects();
    let (dir, _, mut worker) = Fixture::new().start();
    let leaf = dir.path().join("guest-delete-issue-lost.bin");
    fs::write(&leaf, b"still here").unwrap();
    let (session, mut selection) = worker
        .select_mutation_existing(leaf.clone(), scope(Disposition::Delete), [78; 32])
        .unwrap();
    selected(&mut selection);
    let plan = planned(
        &mut worker
            .build_mutation_plan(session, "guest-delete-issue-lost".into(), 0, None)
            .unwrap(),
    );
    let mut issue = worker
        .issue_mutation_guest(session, plan.clone(), [79; 32])
        .unwrap();
    ready(&issue);
    assert!(issue.is_started());
    issue.cancel();
    assert_eq!(issue.read().err(), Some(OwnerCommandError::Unknown));
    assert!(matches!(
        read(&mut worker.prepare_mutation(session, plan.clone()).unwrap()),
        Ok(MutationResponse::Prepared(_))
    ));
    assert_eq!(
        read(
            &mut worker
                .authorize_mutation_guest_execution(session, digest(&plan))
                .unwrap()
        )
        .err(),
        Some(TargetError::Busy)
    );
    assert_eq!(
        read(&mut worker.execute_mutation(session).unwrap()).err(),
        Some(TargetError::Busy)
    );
    assert_eq!(
        read(
            &mut worker
                .issue_mutation_guest(session, plan, [80; 32])
                .unwrap()
        )
        .err(),
        Some(TargetError::AlreadyDispatched)
    );
    assert!(matches!(
        read(&mut worker.release_mutation(session).unwrap()),
        Ok(MutationResponse::Released)
    ));
    assert_eq!(fs::read(&leaf).unwrap(), b"still here");
    assert_eq!(reclaim(&mut worker).disconnect, Ok(()));
}

#[test]
fn foreign_worker_and_expired_owner_cannot_issue_or_use_guest_permit() {
    let _serial = serial_effects();
    let fixture = Fixture::new();
    let clock = Arc::clone(&fixture.clock);
    let (dir, _, mut worker) = fixture.start();
    let (_, _, mut foreign) = Fixture::new().start();
    let leaf = dir.path().join("guest-expired.bin");
    fs::write(&leaf, b"unchanged").unwrap();
    let (session, mut selection) = worker
        .select_mutation_existing(leaf.clone(), scope(Disposition::Delete), [86; 32])
        .unwrap();
    selected(&mut selection);
    let plan = planned(
        &mut worker
            .build_mutation_plan(session, "guest-expired".into(), 0, None)
            .unwrap(),
    );
    let before = foreign.owner_command_usage();
    let bytes = foreign.bytes();
    assert!(matches!(
        foreign.issue_mutation_guest(session, plan.clone(), [87; 32]),
        Err(OwnerCommandError::Closed)
    ));
    assert!(matches!(
        foreign.authorize_mutation_guest_execution(session, digest(&plan)),
        Err(OwnerCommandError::Closed)
    ));
    assert_eq!(foreign.owner_command_usage(), before);
    assert_eq!(foreign.bytes(), bytes);
    approved(
        &mut worker
            .issue_mutation_guest(session, plan.clone(), [87; 32])
            .unwrap(),
    );
    assert!(matches!(
        read(&mut worker.prepare_mutation(session, plan.clone()).unwrap()),
        Ok(MutationResponse::Prepared(_))
    ));
    permitted(
        &mut worker
            .authorize_mutation_guest_execution(session, digest(&plan))
            .unwrap(),
    );
    clock.store(30_001, Ordering::SeqCst);
    assert!(matches!(
        worker.execute_mutation(session),
        Err(OwnerCommandError::Closed)
    ));
    assert_eq!(reclaim(&mut worker).disconnect, Ok(()));
    assert_eq!(reclaim(&mut foreign).disconnect, Ok(()));
    assert_eq!(fs::read(&leaf).unwrap(), b"unchanged");
}

#[test]
fn worker_lifetime_guest_reference_quota_is_128_even_after_release() {
    let _serial = serial_effects();
    let (dir, package, mut worker) = Fixture::new().start();
    let root = dir.path().join("guest-reference-quota");
    fs::create_dir(&root).unwrap();
    for index in 0..=128usize {
        let name = format!("leaf-{index}.bin");
        let relative = RelativeFilePath::parse(&name).unwrap();
        let (session, mut selection) = worker
            .select_mutation_create(
                root.clone(),
                relative.clone(),
                scope(Disposition::Create),
                [index as u8 + 1; 32],
            )
            .unwrap();
        let (reference, expected) = selected(&mut selection);
        assert_eq!(expected, None);
        let plan = request(
            package,
            &format!("guest-reference-{index}"),
            Disposition::Create,
            reference,
            Some(relative),
            None,
            b"",
        );
        let mut issue = worker
            .issue_mutation_guest(session, plan, [index as u8 + 1; 32])
            .unwrap();
        if index < 128 {
            assert_eq!(approved(&mut issue).reference(), [index as u8 + 1; 32]);
        } else {
            assert_eq!(read(&mut issue).err(), Some(TargetError::Limit));
        }
        assert!(matches!(
            read(&mut worker.release_mutation(session).unwrap()),
            Ok(MutationResponse::Released)
        ));
        assert!(!root.join(name).exists());
    }
    let exit = reclaim(&mut worker);
    assert_eq!(exit.disconnect, Ok(()));
    assert!(
        exit.owner
            .host
            .store_local()
            .lookup_io_intent(SUBJECT, "guest-reference-128")
            .unwrap()
            .is_none()
    );
}
