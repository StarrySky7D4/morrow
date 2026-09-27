//! Authoritative draft construction through the private scheduler.
use super::*;

fn build(
    app: &mut Workbench,
    key: &[u8],
    token: &[u8; 32],
    operation: &str,
    length: u64,
    hash: &[u8],
) -> Reader<OwnedSegments> {
    wire_call(app, wire::Action::MutationSubmit, |mut request| {
        request.set_io_key(key);
        let mut c = request.init_mutation_command();
        c.set_kind(9);
        c.set_submission(token);
        c.set_operation_id(operation);
        c.set_content_length(length);
        c.set_content_sha256(hash);
    })
}

#[test]
fn generated_create_and_delete_plans_use_retained_selection_and_execute_explicitly() {
    let _serial = serial_effects();
    for disposition in [Disposition::Create, Disposition::Delete] {
        let mut setup = Setup::new("rust");
        let root = setup.dir.path().join("root");
        fs::create_dir(&root).unwrap();
        let leaf = root.join("planned.bin");
        let bytes = b"generated plan roundtrip";
        let create = disposition == Disposition::Create;
        if !create {
            fs::write(&leaf, b"original").unwrap();
        }
        let revision = setup.options(disposition).revision;
        let started = start(
            &mut setup,
            revision,
            &[31; 32],
            disposition,
            if create { &root } else { &leaf },
            if create { "planned.bin" } else { "" },
        );
        assert_ok(&started);
        let key = row(&started)
            .get_io_state()
            .unwrap()
            .get_key()
            .unwrap()
            .to_vec();
        let task = TaskKey::from_bytes(&key).unwrap();
        let selected = read_result(&mut setup.app, &key, 1);
        let selected = row(&selected).get_mutation_result().unwrap();
        let reference = selected.get_reference().unwrap().try_into().unwrap();
        let expected = if create {
            None
        } else {
            Some(
                selected
                    .get_expected_identity()
                    .unwrap()
                    .try_into()
                    .unwrap(),
            )
        };
        let digest: [u8; 32] = Sha256::digest(bytes).into();
        let hash = if create { &digest[..] } else { &[][..] };
        let length = if create { bytes.len() as u64 } else { 0 };
        let built = build(
            &mut setup.app,
            &key,
            &[32; 32],
            "wire-planned",
            length,
            hash,
        );
        let command = command_id(&built);
        assert_eq!(row(&built).get_mutation_state().unwrap().get_kind(), 9);
        assert_eq!(
            command_id(&build(
                &mut setup.app,
                &key,
                &[32; 32],
                "wire-planned",
                length,
                hash
            )),
            command
        );
        for (operation, changed_length, changed_hash) in [
            ("changed-plan", length, hash),
            ("wire-planned", length + 1, hash),
            ("wire-planned", length, &[9; 32][..]),
        ] {
            assert!(
                !row(&build(
                    &mut setup.app,
                    &key,
                    &[32; 32],
                    operation,
                    changed_length,
                    changed_hash
                ))
                .get_error()
                .unwrap()
                .is_empty()
            );
        }
        let result = read_result(&mut setup.app, &key, command);
        if create {
            export_wire_fixture("planned", &result);
        }
        let result_row = row(&result).get_mutation_result().unwrap();
        assert_eq!(result_row.get_kind(), 11);
        assert_eq!(result_row.get_phase(), 0);
        assert_eq!(result_row.get_effect(), 0);
        assert!(result_row.get_record().unwrap().is_empty());
        assert!(result_row.get_outcome().unwrap().is_empty());
        let plan_bytes = result_row.get_plan().unwrap().to_vec();
        let original = plan(
            setup.digest,
            "wire-planned",
            disposition,
            reference,
            create.then(|| RelativeFilePath::parse("planned.bin").unwrap()),
            expected,
            if create { bytes } else { &[] },
        );
        assert_eq!(
            RequestRecord::decode(&plan_bytes).unwrap().request(),
            original.request()
        );
        assert_eq!(plan_bytes, original.container());
        assert_eq!(leaf.exists(), !create);
        let query = submit(&mut setup.app, &key, &[33; 32], 5, &[], 0, &[]);
        let query = read_result(&mut setup.app, &key, command_id(&query));
        assert_eq!(row(&query).get_mutation_result().unwrap().get_kind(), 6);
        assert!(
            row(&query)
                .get_mutation_result()
                .unwrap()
                .get_record()
                .unwrap()
                .is_empty()
        );
        // A duplicate acknowledges its old command, without constructing a new plan.
        assert_eq!(
            command_id(&build(
                &mut setup.app,
                &key,
                &[32; 32],
                "wire-planned",
                length,
                hash
            )),
            command
        );
        let prepared = submit(&mut setup.app, &key, &[34; 32], 1, &plan_bytes, 0, &[]);
        let prepared = read_result(&mut setup.app, &key, command_id(&prepared));
        assert_eq!(row(&prepared).get_mutation_result().unwrap().get_kind(), 2);
        if create {
            let chunk = submit(&mut setup.app, &key, &[35; 32], 2, &[], 0, bytes);
            let chunk = read_result(&mut setup.app, &key, command_id(&chunk));
            assert_eq!(row(&chunk).get_mutation_result().unwrap().get_kind(), 3);
            let commit = submit(&mut setup.app, &key, &[36; 32], 3, &[], 0, &[]);
            let commit = read_result(&mut setup.app, &key, command_id(&commit));
            assert!(
                row(&commit)
                    .get_mutation_result()
                    .unwrap()
                    .get_durable_content()
            );
        }
        let execute = submit(&mut setup.app, &key, &[37; 32], 4, &[], 0, &[]);
        let execute = read_result(&mut setup.app, &key, command_id(&execute));
        assert_eq!(
            row(&execute).get_mutation_result().unwrap().get_kind(),
            if create { 4 } else { 5 }
        );
        assert_eq!(row(&execute).get_mutation_result().unwrap().get_effect(), 1);
        if create {
            assert_eq!(fs::read(&leaf).unwrap(), bytes);
        } else {
            assert!(!leaf.exists());
        }
        close(&mut setup, &key, task, &[38; 32]);
    }
}

#[test]
fn plan_specification_rejects_mixed_fields_and_invalid_inputs_without_advancing_command() {
    let _serial = serial_effects();
    let mut setup = Setup::new("rust");
    let root = setup.dir.path().join("root");
    fs::create_dir(&root).unwrap();
    let revision = setup.options(Disposition::Create).revision;
    let started = start(
        &mut setup,
        revision,
        &[41; 32],
        Disposition::Create,
        &root,
        "new.bin",
    );
    assert_ok(&started);
    let key = row(&started)
        .get_io_state()
        .unwrap()
        .get_key()
        .unwrap()
        .to_vec();
    let task = TaskKey::from_bytes(&key).unwrap();
    read_result(&mut setup.app, &key, 1);
    for variant in 0..6 {
        let rejected = wire_call(
            &mut setup.app,
            wire::Action::MutationSubmit,
            |mut request| {
                request.set_io_key(&key);
                let mut c = request.init_mutation_command();
                c.set_submission(&[42; 32]);
                c.set_kind(if variant < 3 { 5 } else { 9 });
                match variant {
                    0 => c.set_operation_id("forbidden"),
                    1 => c.set_content_length(1),
                    2 => c.set_content_sha256(&[1; 32]),
                    3 => c.set_plan(&[1]),
                    4 => c.set_bytes(&[1]),
                    _ => c.set_offset(1),
                }
            },
        );
        assert!(!row(&rejected).get_error().unwrap().is_empty());
    }
    for (operation, length, hash) in [
        ("", 1, &[1; 32][..]),
        ("bad\0id", 1, &[1; 32][..]),
        ("valid", u64::MAX, &[1; 32][..]),
        ("valid", 1, &[1; 31][..]),
        ("valid", 1, &[0; 32][..]),
        ("valid", 1, &[][..]),
        ("valid", 0, &[1; 32][..]),
    ] {
        let rejected = build(&mut setup.app, &key, &[43; 32], operation, length, hash);
        assert!(!row(&rejected).get_error().unwrap().is_empty());
    }
    assert_eq!(setup.app.mutation_status(task).unwrap().command, 1);
    assert!(!root.join("new.bin").exists());
    let hash: [u8; 32] = Sha256::digest([]).into();
    let accepted = build(&mut setup.app, &key, &[43; 32], "empty-create", 0, &hash);
    let result = read_result(&mut setup.app, &key, command_id(&accepted));
    assert_eq!(row(&result).get_mutation_result().unwrap().get_kind(), 11);
    close(&mut setup, &key, task, &[44; 32]);
}
