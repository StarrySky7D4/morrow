//! Native Workbench discovery keeps recovery separate from target authority.
use super::*;
use morrow_core::{
    io_evidence::{Kind, Material},
    io_intent::Record,
};
#[test]
fn native_discovery_pages_original_plans_without_target_or_effect_authority() {
    let _serial = serial_effects();
    let mut setup = Setup::new("rust");
    let original = setup.app.local_state().unwrap().host.binding();
    let mut plans = Vec::new();
    for operation in ["a-discovery", "b-discovery"] {
        let leaf = setup.dir.path().join(operation);
        fs::write(&leaf, b"retained").unwrap();
        let key = setup
            .app
            .start_mutation(
                setup.options(Disposition::Delete),
                Selection::Existing(leaf.clone()),
                scope(Disposition::Delete),
            )
            .unwrap();
        let MutationResponse::Selected {
            reference,
            expected_identity,
        } = read(&mut setup.app, key, 1).unwrap()
        else {
            panic!("selection")
        };
        let request = plan(
            setup.digest,
            operation,
            Disposition::Delete,
            reference,
            None,
            expected_identity,
            b"",
        );
        assert!(matches!(
            call(
                &mut setup.app,
                key,
                Action::Prepare(Box::new(request.clone()))
            ),
            Ok(MutationResponse::Prepared(_))
        ));
        assert!(matches!(
            call(&mut setup.app, key, Action::Release),
            Ok(MutationResponse::Released)
        ));
        reclaim(&mut setup.app, key);
        setup.app.acknowledge_io(key).unwrap();
        fs::remove_file(leaf).unwrap();
        plans.push(request);
    }
    let key = setup
        .app
        .start_mutation_discovery(
            setup.options(Disposition::Delete),
            SUBJECT.into(),
            Disposition::Delete,
            1,
        )
        .unwrap();
    assert!(!setup.app.mutation_status(key).unwrap().selected);
    // A stale wire command must not consume the pending discovery page.
    let mut frame = capnp::message::Builder::new_default();
    let mut request = frame.init_root::<crate::host_capnp::request::Builder>();
    request.set_version(1);
    request.set_digest(&crate::protocol::digest());
    request.set_action(crate::host_capnp::Action::MutationRead);
    request.set_io_key(key.as_bytes());
    request.set_mutation_command_id(999);
    let response = crate::protocol::respond(
        &mut setup.app,
        &capnp::serialize::write_message_to_words(&frame),
    )
    .unwrap();
    let response = capnp::serialize::read_message(
        &mut response.as_slice(),
        capnp::message::ReaderOptions::new(),
    )
    .unwrap();
    assert!(
        !response
            .get_root::<crate::host_capnp::response::Reader>()
            .unwrap()
            .get_error()
            .unwrap()
            .is_empty()
    );
    let MutationResponse::Plans {
        plans: first,
        scanned,
        done,
        ..
    } = read(&mut setup.app, key, 1).unwrap()
    else {
        panic!("page")
    };
    assert_eq!(scanned, 1);
    assert!(!done);
    assert_eq!(first[0].container(), plans[0].container());
    for action in [
        Action::Query,
        Action::Execute,
        Action::Prepare(Box::new(plans[0].clone())),
        Action::BuildPlan {
            operation_id: "forbidden".into(),
            content_length: 0,
            content_sha256: None,
        },
        Action::CancelPlan,
    ] {
        assert!(setup.app.request_mutation(key, action).is_err());
    }
    let command = setup
        .app
        .submit_mutation(key, [91; 32], Action::NextPlans { scan_limit: 1 })
        .unwrap();
    assert_eq!(
        setup
            .app
            .submit_mutation(key, [91; 32], Action::NextPlans { scan_limit: 1 })
            .unwrap(),
        command
    );
    assert!(
        setup
            .app
            .submit_mutation(key, [91; 32], Action::NextPlans { scan_limit: 2 })
            .is_err()
    );
    let MutationResponse::Plans {
        plans: last,
        scanned,
        done,
        ..
    } = read(&mut setup.app, key, command).unwrap()
    else {
        panic!("last page")
    };
    assert_eq!(scanned, 1);
    assert!(done);
    assert_eq!(last[0].container(), plans[1].container());
    assert!(setup.app.mutation_status(key).unwrap().terminal);
    assert!(
        setup
            .app
            .request_mutation(key, Action::NextPlans { scan_limit: 1 })
            .is_err()
    );
    assert!(matches!(
        call(&mut setup.app, key, Action::Release),
        Ok(MutationResponse::Released)
    ));
    reclaim(&mut setup.app, key);
    setup.app.acknowledge_io(key).unwrap();
    assert_eq!(setup.app.local_state().unwrap().host.binding(), original);
    assert!(setup.app.mutation_status(key).is_err());
}
#[test]
fn discovery_authority_rejection_and_cancelled_page_do_not_restore_a_selection() {
    let _serial = serial_effects();
    let mut setup = Setup::new("rust");

    assert!(
        setup
            .app
            .start_mutation_discovery(
                setup.options(Disposition::Create),
                SUBJECT.into(),
                Disposition::Delete,
                1
            )
            .is_err()
    );

    assert!(
        setup
            .app
            .start_mutation_discovery(
                setup.options(Disposition::Delete),
                "bad/subject".into(),
                Disposition::Delete,
                1
            )
            .is_err()
    );
    assert!(
        setup
            .app
            .start_mutation_discovery(
                setup.options(Disposition::Delete),
                SUBJECT.into(),
                Disposition::Delete,
                9
            )
            .is_err()
    );
    let request = plan(
        setup.digest,
        "stored-plan",
        Disposition::Delete,
        [7; 32],
        None,
        Some([8; 32]),
        b"",
    );
    let prepared = Record::prepared(request.command().unwrap()).unwrap();
    let material = Material::encode(
        Kind::Request,
        "stored-plan",
        SUBJECT,
        prepared.command().request_sha256,
        request.container(),
    )
    .unwrap();
    setup
        .app
        .local_state_mut()
        .unwrap()
        .host
        .store_local_mut()
        .prepare_file_mutation_local_authorized(&prepared, &material, || Ok(()))
        .unwrap();
    let key = setup
        .app
        .start_mutation_discovery(
            setup.options(Disposition::Delete),
            SUBJECT.into(),
            Disposition::Delete,
            1,
        )
        .unwrap();
    setup.app.cancel_mutation_command(key, 1).unwrap();
    assert!(read(&mut setup.app, key, 1).is_err());
    assert!(setup.app.mutation_status(key).unwrap().terminal);
    assert!(!setup.app.mutation_status(key).unwrap().reconcile_required);
    assert!(!setup.app.mutation_status(key).unwrap().selected);
    assert!(
        setup
            .app
            .request_mutation(key, Action::NextPlans { scan_limit: 1 })
            .is_err()
    );
    setup.app.cancel_io(key).unwrap();
    reclaim(&mut setup.app, key);
    setup.app.acknowledge_io(key).unwrap();
}
