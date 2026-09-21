use super::*;
use morrow_network_node::managed_http::HttpRouteSet;

#[tokio::test]
async fn persistent_reference_requires_fresh_original_grant_after_disconnect() {
    let server = Server::new(Some(raw_response("200 OK", b"live")), Duration::ZERO).await;
    let mut setup = stored_setup(false);
    setup.persistent = true;
    let mut run = Running::stored(approval(&server.origin), false, setup).unwrap();
    let wire = STORED_ENDPOINT
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<String>();
    assert_eq!(run.endpoint.endpoint_reference(), wire);
    let (_, mut job) = run.submit(&submission(&run.endpoint, "persistent-first"));
    assert_eq!(consume(&mut job).await.task.execution.outcome, Ok(0));
    let stale = HttpRouteSet::new(
        vec![run.endpoint.clone()],
        tokio::runtime::Handle::current(),
    )
    .unwrap();
    let directory = stale.resources([3; 32]).unwrap();
    let directory =
        morrow_core::service_resources::Directory::decode(&directory.encode().unwrap()).unwrap();
    assert_eq!(directory.endpoints[0].reference, wire);
    let mut host = run.finish().await;
    let old_approval = host
        .store_local()
        .lookup_io_intent(ID, "persistent-first")
        .unwrap()
        .unwrap()
        .command()
        .approval_sha256;
    let original = host.binding();
    let resolved =
        StoredHttpEndpoint::resolve(host.store_local_mut(), &STORED_ENDPOINT, || 1000).unwrap();
    let digest = run._manager.selection(ID).unwrap().digest;
    let instance = run._manager.connect(ID, &mut host).unwrap();
    let binding = run
        ._manager
        .bind_io(
            &host,
            &instance,
            digest,
            run._manager.revision(),
            &BTreeSet::from([IoCapability::HttpRequest]),
            100,
            2,
        )
        .unwrap();
    let fresh = resolved
        .approve_persistent(
            &run._manager,
            &host,
            &instance,
            &binding,
            [9; 32],
            2,
            |_| panic!("no credential provider needed"),
        )
        .unwrap();
    assert_eq!(fresh.endpoint_reference(), wire);
    run.worker = IoWorker::spawn_managed_owned(
        &run._manager,
        HttpOwner { original, host },
        instance,
        binding,
        || 2,
        1,
        JobLimits::new(1, 1024 * 1024, 4 * 1024 * 1024).unwrap(),
    )
    .unwrap();
    let mut old_metadata_request = submission(&fresh, "persistent-stale");
    old_metadata_request.endpoint = directory.endpoints[0].reference.as_bytes().to_vec();
    let request = Request::encode_http_submit(1, &old_metadata_request).unwrap();
    let mut job = run
        .worker
        .submit_brokered(request.bytes().to_vec(), Box::new(stale), WAIT)
        .unwrap();
    assert!(consume(&mut job).await.task.execution.outcome.is_err());
    assert_eq!(server.calls.load(Ordering::SeqCst), 1);
    run.endpoint = fresh;
    let (_, mut job) = run.submit(&submission(&run.endpoint, "persistent-second"));
    assert_eq!(consume(&mut job).await.task.execution.outcome, Ok(0));
    run.endpoint.revoke();
    let (_, mut job) = run.submit(&submission(&run.endpoint, "persistent-revoked"));
    assert!(consume(&mut job).await.task.execution.outcome.is_err());
    assert_eq!(server.calls.load(Ordering::SeqCst), 2);
    let host = run.finish().await;
    let current = host
        .store_local()
        .lookup_io_intent(ID, "persistent-second")
        .unwrap()
        .unwrap();
    assert_ne!(current.command().approval_sha256, old_approval);
    for id in ["persistent-stale", "persistent-revoked"] {
        assert!(
            host.store_local()
                .lookup_io_intent(ID, id)
                .unwrap()
                .is_none()
        );
    }
}

#[tokio::test]
async fn ephemeral_stored_approval_keeps_session_reference() {
    let server = Server::new(Some(raw_response("200 OK", b"unused")), Duration::ZERO).await;
    let mut run = Running::stored(approval(&server.origin), false, stored_setup(false)).unwrap();
    let persistent = STORED_ENDPOINT
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<String>();
    assert_ne!(run.endpoint.endpoint_reference(), persistent);
    run.finish().await;
}

#[tokio::test]
async fn approved_directory_delivers_only_credential_reference_for_real_http() {
    let mut server = Server::new(Some(raw_response("200 OK", b"live")), Duration::ZERO).await;
    let mut setup = stored_setup(true);
    setup.persistent = true;
    let mut run = Running::stored(approval(&server.origin), true, setup).unwrap();
    let routes = HttpRouteSet::new(
        vec![run.endpoint.clone()],
        tokio::runtime::Handle::current(),
    )
    .unwrap();
    let directory = routes.resources([4; 32]).unwrap();
    let header = directory.to_header().unwrap();
    let directory = morrow_core::service_resources::Directory::from_headers(&[header])
        .unwrap()
        .unwrap();
    let endpoint = &directory.endpoints[0];
    assert_eq!(
        endpoint.credential,
        STORED_CREDENTIAL
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>()
            .as_bytes()
    );
    assert_eq!(endpoint.methods, vec!["GET", "HEAD", "POST"]);
    assert_eq!(endpoint.max_request_bytes, 65536);
    let mut input = submission(&run.endpoint, "directory-credential");
    input.endpoint = endpoint.reference.as_bytes().to_vec();
    input.credential = endpoint.credential.clone();
    let (_, mut job) = run.submit(&input);
    assert_eq!(consume(&mut job).await.task.execution.outcome, Ok(0));
    assert!(
        !directory
            .encode()
            .unwrap()
            .windows(TOKEN.len())
            .any(|w| w == TOKEN.as_bytes())
    );
    let sent = server.request().await;
    assert!(String::from_utf8_lossy(&sent).contains(&format!("authorization: Bearer {TOKEN}")));
    assert_eq!(server.calls.load(Ordering::SeqCst), 1);
    run.endpoint.revoke();
    input.operation_id = b"directory-after-revoke".to_vec();
    let (_, mut job) = run.submit(&input);
    assert!(consume(&mut job).await.task.execution.outcome.is_err());
    assert_eq!(server.calls.load(Ordering::SeqCst), 1);
    let host = run.finish().await;
    assert!(
        host.store_local()
            .lookup_io_intent(ID, "directory-after-revoke")
            .unwrap()
            .is_none()
    );
}

#[tokio::test]
async fn selection_digest_binds_complete_records_and_is_order_independent() {
    let server = Server::new(Some(raw_response("200 OK", b"unused")), Duration::ZERO).await;
    let mut run = Running::stored(approval(&server.origin), true, stored_setup(true)).unwrap();
    let mut host = run.finish().await;
    drop(run.endpoint);
    let original = host
        .store_local()
        .load_outbound_authority(&STORED_ENDPOINT)
        .unwrap()
        .unwrap();
    let credential = host
        .store_local()
        .load_outbound_authority(&STORED_CREDENTIAL)
        .unwrap()
        .unwrap();
    let mut second = original.value().clone();
    second.reference = vec![33; 32];
    let second = StoredRecord::encode(second).unwrap();
    host.store_local_mut()
        .save_outbound_authority_local(&second, 0)
        .unwrap();
    let resolve = |host: &mut HostRuntime, keys: &[[u8; 32]]| {
        keys.iter()
            .map(|key| StoredHttpEndpoint::resolve(host.store_local_mut(), key, || 1000).unwrap())
            .collect::<Vec<_>>()
    };
    let selected = resolve(&mut host, &[STORED_ENDPOINT, [33; 32]]);
    let first = StoredHttpEndpoint::selection_digest(&selected).unwrap();
    assert!(first.is_some());
    let reversed = resolve(&mut host, &[[33; 32], STORED_ENDPOINT]);
    assert_eq!(
        first,
        StoredHttpEndpoint::selection_digest(&reversed).unwrap()
    );
    let duplicates = resolve(&mut host, &[STORED_ENDPOINT, STORED_ENDPOINT]);
    assert!(matches!(
        StoredHttpEndpoint::selection_digest(&duplicates),
        Err(morrow_network_node::Error::Invalid)
    ));
    let too_many = resolve(&mut host, &[STORED_ENDPOINT; 9]);
    assert!(matches!(
        StoredHttpEndpoint::selection_digest(&too_many),
        Err(morrow_network_node::Error::Limit)
    ));
    assert_eq!(StoredHttpEndpoint::selection_digest(&[]).unwrap(), None);
    drop((selected, reversed, duplicates, too_many));
    let mut modified = credential.value().clone();
    modified.revision += 1;
    modified.expires_ms += 1;
    let Some(outbound::record::Kind::Credential(secret)) = modified.kind.as_mut() else {
        unreachable!()
    };
    secret.ciphertext.push(4);
    host.store_local_mut()
        .save_outbound_authority_local(&StoredRecord::encode(modified).unwrap(), 1)
        .unwrap();
    let changed = resolve(&mut host, &[STORED_ENDPOINT, [33; 32]]);
    assert_ne!(
        first,
        StoredHttpEndpoint::selection_digest(&changed).unwrap()
    );
    drop(changed);
    let mut modified = original.value().clone();
    modified.revision += 1;
    let Some(outbound::record::Kind::Endpoint(value)) = modified.kind.as_mut() else {
        unreachable!()
    };
    value.methods = vec!["GET".into()];
    let before =
        StoredHttpEndpoint::selection_digest(&resolve(&mut host, &[STORED_ENDPOINT])).unwrap();
    host.store_local_mut()
        .save_outbound_authority_local(&StoredRecord::encode(modified).unwrap(), 1)
        .unwrap();
    assert_ne!(
        before,
        StoredHttpEndpoint::selection_digest(&resolve(&mut host, &[STORED_ENDPOINT])).unwrap()
    );
}

#[tokio::test]
async fn stored_unrelated_outbound_reference_mutation_preserves_request_ready() {
    for ready_first in [false, true] {
        let mut server = Server::new(
            Some(raw_response("200 OK", b"delivered")),
            if ready_first {
                Duration::ZERO
            } else {
                Duration::from_millis(150)
            },
        )
        .await;
        let mut run = Running::stored(approval(&server.origin), true, stored_setup(true)).unwrap();
        let mut input = submission(&run.endpoint, "stored-unrelated");
        input.credential = credential_wire_reference();
        let (_, mut job) = run.submit(&input);
        let _ = server.request().await;
        if ready_first {
            ready(&mut job).await;
        }
        let observer =
            Store::open_existing(&run._dir.path().join("db"), EventBudget::default()).unwrap();
        let mut value = observer
            .load_outbound_authority(&STORED_CREDENTIAL)
            .unwrap()
            .unwrap()
            .value()
            .clone();
        value.reference = vec![99; 32];
        value.revision = 1;
        value.disabled = true;
        let value = StoredRecord::encode(value).unwrap();
        let mut update = run
            .worker
            .update_service(ServiceUpdate::Outbound {
                value,
                expected_revision: 0,
            })
            .unwrap();
        let report = consume(&mut job).await;
        assert!(report.task.execution.outcome.is_ok());
        assert!(report.http_response.is_some());
        tokio::time::timeout(WAIT, async {
            loop {
                if let Some(result) = update.read().unwrap() {
                    result.unwrap();
                    break;
                }
                tokio::time::sleep(Duration::from_millis(1)).await;
            }
        })
        .await
        .unwrap();
        assert_eq!(server.calls.load(Ordering::SeqCst), 1);
        let _ = run.finish().await;
    }
}
