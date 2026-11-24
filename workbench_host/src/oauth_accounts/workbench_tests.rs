use super::{Provider, Service, Workbench, browse, service, test_plugin};
use crate::{host_capnp as wire, oauth_control::Selection, protocol};
use capnp::{message::Builder, serialize};
use std::time::{Duration, Instant};
use tokio::runtime::Runtime;
use url::Url;
use wire::OAuthAction as Action;
#[derive(Default)]
struct View {
    services: Vec<([u8; 32], String, String)>,
    accounts: Vec<([u8; 32], u64, u16)>,
    phase: u16,
    url: String,
    account: Vec<u8>,
    key: Vec<u8>,
    error: String,
}
fn call(
    w: &mut Workbench,
    s: &Selection,
    action: Action,
    fill: impl FnOnce(wire::o_auth_control::Builder<'_>),
) -> View {
    let mut message = Builder::new_default();
    let mut r = message.init_root::<wire::request::Builder>();
    r.set_action(wire::Action::OauthControl);
    r.set_version(1);
    r.set_digest(&protocol::digest());
    r.set_id(s.package.as_str());
    r.set_sha256(&s.digest);
    r.set_revision(s.revision);
    let mut p = r.init_oauth_control();
    p.set_action(action);
    fill(p);
    let bytes = protocol::respond(w, &serialize::write_message_to_words(&message)).unwrap();
    for prefix in [
        b"fixture-access-".as_slice(),
        b"fixture-refresh-".as_slice(),
    ] {
        assert!(!bytes.windows(prefix.len()).any(|v| v == prefix));
    }
    let frame = serialize::read_message(&mut &bytes[..], Default::default()).unwrap();
    let r = frame.get_root::<wire::response::Reader>().unwrap();
    let v = r.get_oauth_view().unwrap();
    View {
        error: r.get_error().unwrap().to_str().unwrap().into(),
        services: v
            .get_services()
            .unwrap()
            .iter()
            .map(|v| {
                (
                    v.get_key().unwrap().try_into().unwrap(),
                    v.get_token_endpoint()
                        .unwrap()
                        .to_str()
                        .unwrap()
                        .to_string(),
                    v.get_client_id().unwrap().to_str().unwrap().to_string(),
                )
            })
            .collect(),
        accounts: v
            .get_accounts()
            .unwrap()
            .iter()
            .map(|v| {
                (
                    v.get_reference().unwrap().try_into().unwrap(),
                    v.get_revision(),
                    v.get_state(),
                )
            })
            .collect(),
        url: v.get_browser_url().unwrap().to_str().unwrap().into(),
        phase: v.get_phase(),
        account: v.get_account().unwrap().to_vec(),
        key: v.get_key().unwrap().to_vec(),
    }
}
fn login(w: &mut Workbench, s: &Selection, key: [u8; 32], nonce: u8) -> View {
    call(w, s, Action::Login, |mut p| {
        p.set_service(&key);
        p.set_days(7);
        p.set_key(&[nonce; 32]);
    })
}
fn query(w: &mut Workbench, s: &Selection, key: [u8; 32], nonce: u8, action: Action) -> View {
    call(w, s, action, |mut p| {
        p.set_service(&key);
        p.set_key(&[nonce; 32]);
    })
}
fn completed(w: &mut Workbench, s: &Selection, key: [u8; 32], nonce: u8) -> View {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let v = query(w, s, key, nonce, Action::Poll);
        assert!(v.error.is_empty(), "{}", v.error);
        assert!(v.url.is_empty());
        if v.phase != 1 {
            return v;
        }
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(10));
    }
}
fn confirm(rt: &Runtime, raw: &str, name: &str) {
    rt.block_on(async {
        let first = browse(&Url::parse(raw).unwrap(), Some(name)).await;
        assert_eq!(first.status, 307);
        let callback = Url::parse(
            &first
                .headers
                .iter()
                .find(|(k, _)| k == "location")
                .unwrap()
                .1,
        )
        .unwrap();
        assert_eq!(browse(&callback, None).await.status, 200);
    });
}
fn install(w: &mut Workbench, dir: &std::path::Path) -> Selection {
    let pkg = test_plugin::install(w, dir, "test.account-entry", "1.0.0", true);
    let id = &pkg.manifest().package_id;
    w.configure_external(id, &pkg.digest(), test_plugin::revision(w), &[], true)
        .unwrap();
    test_plugin::approve(w, &pkg, true);
    Selection {
        package: id.clone(),
        digest: pkg.digest(),
        revision: test_plugin::revision(w),
    }
}
fn register(w: &mut Workbench, s: &Selection, service: &Service) -> [u8; 32] {
    w.bind_approved_oauth_service(
        s.clone(),
        "Test accounts",
        Provider::fixture(&service.origin),
    )
    .unwrap()
}
#[test]
fn sign_out_while_the_original_guest_request_is_waiting_is_account_scoped_and_durable() {
    let rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(1)
        .enable_all()
        .build()
        .unwrap();
    for language in ["rust", "c", "cpp"] {
        for active in 0..2 {
            let (service, server) = rt.block_on(super::service_with_lifetime(90));
            let directory = tempfile::tempdir().unwrap();
            let mut w = Workbench::open_managed(directory.path(), None).unwrap();
            w.bind_supervised_channel_owner().unwrap();
            let s = install_frozen(&mut w, language);
            let provider = register(&mut w, &s, &service);
            let mut accounts = vec![];
            for (i, name) in ["alpha", "beta"].iter().enumerate() {
                let login_id = 30 + i as u8;
                let start = login(&mut w, &s, provider, login_id);
                confirm(&rt, &start.url, name);
                let row = completed(&mut w, &s, provider, login_id).accounts[0];
                let endpoint = account_endpoint(&mut w, &s, &service.origin, row.0);
                accounts.push((row, endpoint));
            }
            assert_eq!(
                w.oauth_account_page(&s, provider, None, None)
                    .unwrap()
                    .0
                    .len(),
                2
            );
            let mut request = resource_request(&s, &accounts[active].1, 81);
            request.target = "/waiting".into();
            let task = w.start_http(request).unwrap();
            rt.block_on(async {
                tokio::time::timeout(Duration::from_secs(4), service.arrived.notified())
                    .await
                    .unwrap()
            });
            assert!(w.local_state().is_err());
            let (reference, revision, _) = accounts[0].0;
            let withdraw = |w: &mut Workbench, expected, nonce: u8| {
                call(w, &s, Action::Logout, |mut p| {
                    p.set_service(&provider);
                    p.set_account(&reference);
                    p.set_expected_revision(expected);
                    p.set_key(&[nonce; 32]);
                })
            };
            assert!(!withdraw(&mut w, revision + 1, 82).error.is_empty());
            let waiting = withdraw(&mut w, revision, 83);
            assert_eq!(waiting.phase, 1, "{}", waiting.error);
            assert!(waiting.accounts.is_empty());
            assert_eq!(waiting.account, reference);
            assert!(!withdraw(&mut w, revision, 83).error.is_empty());
            let mut other_selection = s.clone();
            other_selection.revision += 1;
            assert!(
                !query(&mut w, &other_selection, provider, 83, Action::Poll)
                    .error
                    .is_empty()
            );
            service.release.notify_one();
            crate::http_tasks_tests::ready(&mut w, task);
            let response = w.read_io(task, 512 * 1024).unwrap().unwrap();
            if active == 0 {
                assert!(
                    response.unknown
                        || response.cancelled
                        || response.task.execution.outcome.is_err()
                );
                assert!(
                    response
                        .http_response
                        .as_ref()
                        .is_none_or(|r| r.http_status != 200 && r.body.is_empty())
                );
            } else {
                assert!(
                    !response.unknown
                        && !response.cancelled
                        && response.task.execution.outcome.is_ok()
                );
                let http = response.http_response.unwrap();
                assert_eq!((http.http_status, http.body), (200, b"beta".to_vec()));
            }
            crate::http_tasks_tests::reclaimed(&mut w, task);
            w.acknowledge_io(task).unwrap();
            let done = completed(&mut w, &s, provider, 83);
            assert_eq!(done.phase, 2);
            assert_eq!(done.accounts, [(reference, revision + 1, 4)]);
            assert_eq!(
                query(&mut w, &s, provider, 83, Action::Cancel).accounts,
                done.accounts
            );
            assert!(!withdraw(&mut w, revision, 83).error.is_empty());
            w.finish().unwrap();
            drop(w);
            let mut w = Workbench::open_managed(directory.path(), None).unwrap();
            w.bind_supervised_channel_owner().unwrap();
            assert_eq!(register(&mut w, &s, &service), provider);
            let missing = query(&mut w, &s, provider, 83, Action::Poll);
            assert_eq!(missing.phase, 5);
            assert!(missing.account.is_empty() && missing.accounts.is_empty());
            let rows = w.oauth_account_page(&s, provider, None, None).unwrap().0;
            assert!(rows.iter().any(|r| r.reference == reference
                && r.revision == revision + 1
                && r.state == super::State::SignedOut));
            assert!(
                w.start_http(resource_request(&s, &accounts[0].1, 85))
                    .is_err()
            );
            resource_call(&mut w, &s, &accounts[1].1, 86, "beta");
            assert_eq!(
                service.data.lock().unwrap().resource_accounts,
                [Some(["alpha", "beta"][active].into()), Some("beta".into())]
            );
            w.finish().unwrap();
            server.abort();
        }
    }
}
#[test]
fn original_windows_control_two_accounts_reopen_refresh_signout_and_cancel() {
    let rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(1)
        .enable_all()
        .build()
        .unwrap();
    let (service, server) = rt.block_on(service());
    let directory = tempfile::tempdir().unwrap();
    let mut w = Workbench::open_managed(directory.path(), None).unwrap();
    let s = install(&mut w, directory.path());
    assert!(!call(&mut w, &s, Action::Services, |_| {}).error.is_empty());
    w.bind_supervised_channel_owner().unwrap();
    assert!(
        call(&mut w, &s, Action::Services, |_| {})
            .services
            .is_empty()
    );
    assert!(!login(&mut w, &s, [54; 32], 10).error.is_empty());
    let key = register(&mut w, &s, &service);
    assert_eq!(
        call(&mut w, &s, Action::Services, |_| {}).services,
        vec![(
            key,
            format!("{}/token", service.origin),
            "morrow-windows-test".into()
        )]
    );
    let mut refs = vec![];
    for (name, nonce) in [("alpha", 11), ("beta", 12)] {
        let start = login(&mut w, &s, key, nonce);
        assert!(start.error.is_empty(), "{}", start.error);
        assert_eq!((start.phase, start.key.as_slice()), (1, &[nonce; 32][..]));
        assert!(!start.url.is_empty());
        assert!(!login(&mut w, &s, key, nonce).error.is_empty());
        assert!(query(&mut w, &s, key, nonce, Action::Poll).url.is_empty());
        confirm(&rt, &start.url, name);
        let v = completed(&mut w, &s, key, nonce);
        assert_eq!(v.phase, 2);
        assert_eq!(v.accounts[0].2, 1);
        assert_eq!(v.account, v.accounts[0].0);
        refs.push(v.accounts[0].0);
    }
    assert_ne!(refs[0], refs[1]);
    let pending = login(&mut w, &s, key, 13);
    assert_eq!(pending.phase, 1);
    assert_eq!(query(&mut w, &s, key, 13, Action::Cancel).phase, 3);
    assert_eq!(service.data.lock().unwrap().exchanges, 2);
    w.finish().unwrap();
    drop(w);
    let mut w = Workbench::open_managed(directory.path(), None).unwrap();
    w.bind_supervised_channel_owner().unwrap();
    assert_eq!(register(&mut w, &s, &service), key);
    let prior = query(&mut w, &s, key, 13, Action::Poll);
    assert!(prior.error.is_empty());
    assert_eq!(prior.phase, 5);
    assert!(prior.account.is_empty() && prior.url.is_empty());
    let list = call(&mut w, &s, Action::Accounts, |mut p| p.set_service(&key));
    assert!(list.error.is_empty(), "{}", list.error);
    assert_eq!(list.accounts.len(), 2);
    let current = call(&mut w, &s, Action::Refresh, |mut p| {
        p.set_service(&key);
        p.set_account(&refs[1]);
        p.set_key(&[20; 32]);
    });
    assert_eq!(current.phase, 2);
    assert_eq!(
        query(&mut w, &s, key, 20, Action::Poll).accounts,
        current.accounts
    );
    assert_eq!(service.data.lock().unwrap().rotations, 0);
    std::thread::sleep(Duration::from_millis(3100));
    let refresh = call(&mut w, &s, Action::Refresh, |mut p| {
        p.set_service(&key);
        p.set_account(&refs[0]);
        p.set_key(&[21; 32]);
    });
    assert_eq!(refresh.phase, 1, "{}", refresh.error);
    assert_eq!(completed(&mut w, &s, key, 21).accounts[0], (refs[0], 3, 1));
    assert_eq!(service.data.lock().unwrap().rotations, 1);
    let stale = call(&mut w, &s, Action::Logout, |mut p| {
        p.set_service(&key);
        p.set_account(&refs[0]);
        p.set_expected_revision(1);
    });
    assert!(!stale.error.is_empty());
    let pending = login(&mut w, &s, key, 22);
    assert_eq!(pending.phase, 1);
    w.configure_external_io(&s.package, &s.digest, s.revision, &[])
        .unwrap();
    assert_eq!(query(&mut w, &s, key, 22, Action::Poll).phase, 3);
    let updated = Selection {
        revision: test_plugin::revision(&w),
        ..s.clone()
    };
    assert!(!login(&mut w, &updated, key, 23).error.is_empty());
    let logout = call(&mut w, &updated, Action::Logout, |mut p| {
        p.set_service(&key);
        p.set_account(&refs[0]);
        p.set_expected_revision(3);
    });
    assert_eq!(logout.accounts, vec![(refs[0], 4, 4)]);
    let final_page = call(&mut w, &updated, Action::Accounts, |mut p| {
        p.set_service(&key)
    });
    assert!(final_page.accounts.contains(&(refs[1], 1, 1)));
    assert_eq!(service.data.lock().unwrap().exchanges, 2);
    w.finish().unwrap();
    server.abort();
}

#[test]
fn losing_the_selected_manager_closes_the_pending_browser_callback_before_poll() {
    let rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(1)
        .enable_all()
        .build()
        .unwrap();
    let (service, server) = rt.block_on(service());
    let directory = tempfile::tempdir().unwrap();
    let mut w = Workbench::open_managed(directory.path(), None).unwrap();
    let s = install(&mut w, directory.path());
    w.bind_supervised_channel_owner().unwrap();
    let key = register(&mut w, &s, &service);
    let start = login(&mut w, &s, key, 20);
    assert_eq!(start.phase, 1);
    let url = Url::parse(&start.url).unwrap();
    let redirect = url
        .query_pairs()
        .find(|(name, _)| name == "redirect_uri")
        .unwrap()
        .1;
    let listener = Url::parse(&redirect).unwrap();
    let address = (std::net::Ipv4Addr::LOCALHOST, listener.port().unwrap());
    assert!(std::net::TcpListener::bind(address).is_err());
    drop(w.local_state_mut().unwrap().manager.take().unwrap());
    let deadline = Instant::now() + Duration::from_secs(3);
    while std::net::TcpListener::bind(address).is_err() {
        assert!(
            Instant::now() < deadline,
            "the original callback is still open"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(query(&mut w, &s, key, 20, Action::Poll).phase, 3);
    assert_eq!(service.data.lock().unwrap().exchanges, 0);
    server.abort();
}

fn resource_request(
    s: &Selection,
    endpoint: &crate::endpoint_control::EndpointInfo,
    nonce: u8,
) -> crate::http_tasks::HttpStart {
    crate::http_tasks::HttpStart {
        submission: [nonce; 32],
        endpoint: endpoint.reference,
        endpoint_revision: endpoint.revision,
        package_digest: s.digest,
        registry_revision: s.revision,
        method: "GET".into(),
        target: "/resource".into(),
        headers: vec![],
        body: vec![],
        timeout_ms: 5000,
    }
}
fn resource_call(
    w: &mut Workbench,
    s: &Selection,
    endpoint: &crate::endpoint_control::EndpointInfo,
    nonce: u8,
    expected: &str,
) {
    use morrow_core::{
        io::{Request, Response},
        io_evidence::Kind,
    };
    let key = w.start_http(resource_request(s, endpoint, nonce)).unwrap();
    crate::http_tasks_tests::ready(w, key);
    let report = w.read_io(key, 512 * 1024).unwrap().unwrap();
    assert!(!report.unknown && !report.cancelled && report.task.execution.outcome.is_ok());
    let http = report.http_response.unwrap();
    assert_eq!(http.http_status, 200);
    assert_eq!(http.body, expected.as_bytes());
    crate::http_tasks_tests::reclaimed(w, key);
    let storage = w.local_state().unwrap().host.store_local();
    let id = format!("{nonce:02x}").repeat(32);
    let req = storage
        .io_material(&s.package, &id, Kind::Request)
        .unwrap()
        .unwrap();
    let resp = storage
        .io_material(&s.package, &id, Kind::Response)
        .unwrap()
        .unwrap();
    assert_eq!(
        Response::decode_http(&Request::decode(req.payload()).unwrap(), resp.payload())
            .unwrap()
            .body,
        expected.as_bytes()
    );
    for bytes in [req.payload(), resp.payload()] {
        for prefix in [b"fixture-access".as_slice(), b"fixture-refresh".as_slice()] {
            assert!(!bytes.windows(prefix.len()).any(|w| w == prefix));
        }
    }
    w.acknowledge_io(key).unwrap();
}
fn install_frozen(w: &mut Workbench, language: &str) -> Selection {
    use morrow_core::plugin_package::catalog;
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(format!(
        "../sdk/compat/transport-v1-rc1/{language}-io.mplugin"
    ));
    let pkg = catalog::read_file(&path).unwrap();
    let id = pkg.manifest().package_id.clone();
    w.import_plugin(&path, &pkg.digest(), test_plugin::revision(w))
        .unwrap();
    w.configure_external(&id, &pkg.digest(), test_plugin::revision(w), &[], true)
        .unwrap();
    test_plugin::approve(w, &pkg, true);
    Selection {
        package: id,
        digest: pkg.digest(),
        revision: test_plugin::revision(w),
    }
}
fn account_endpoint(
    w: &mut Workbench,
    s: &Selection,
    origin: &str,
    account: [u8; 32],
) -> crate::endpoint_control::EndpointInfo {
    w.save_endpoint(crate::endpoint_control::EndpointUpdate {
        reference: vec![],
        expected_revision: 0,
        registry_revision: s.revision,
        lifetime_days: 1,
        policy: morrow_core::outbound_authority::proto::Endpoint {
            package_id: s.package.clone(),
            package_sha256: s.digest.into(),
            origin: origin.into(),
            profile: 2,
            methods: vec!["GET".into()],
            credential_reference: account.to_vec(),
            root_certificate: vec![],
            max_request_bytes: 65536,
            max_response_bytes: 65536,
            max_header_bytes: 16384,
            max_concurrent: 1,
            timeout_ms: 5000,
            max_frame_bytes: 128 * 1024,
        },
    })
    .unwrap()
}
#[test]
fn frozen_guests_choose_two_signed_in_accounts_on_the_same_origin_and_withdraw_one() {
    let rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(1)
        .enable_all()
        .build()
        .unwrap();
    for language in ["rust", "c", "cpp"] {
        let (service, server) = rt.block_on(super::service_with_lifetime(5));
        let directory = tempfile::tempdir().unwrap();
        let mut w = Workbench::open_managed(directory.path(), None).unwrap();
        w.bind_supervised_channel_owner().unwrap();
        let s = install_frozen(&mut w, language);
        let service_id = register(&mut w, &s, &service);
        let mut accounts = vec![];
        for (i, name) in ["alpha", "beta"].into_iter().enumerate() {
            let n = 30 + i as u8;
            let start = login(&mut w, &s, service_id, n);
            assert_eq!(start.phase, 1, "{}", start.error);
            confirm(&rt, &start.url, name);
            let done = completed(&mut w, &s, service_id, n);
            assert_eq!(done.phase, 2);
            let account = done.accounts[0];
            let ep = account_endpoint(&mut w, &s, &service.origin, account.0);
            resource_call(&mut w, &s, &ep, 40 + i as u8, name);
            accounts.push((account, ep));
        }
        let listed = w.oauth_account_page(&s, service_id, None, None).unwrap().0;
        assert_eq!(listed.len(), 2);
        let expires = listed
            .iter()
            .map(|entry| entry.access_expires_ms.unwrap())
            .max()
            .unwrap();
        w.finish().unwrap();
        drop(w);
        let mut w = Workbench::open_managed(directory.path(), None).unwrap();
        w.bind_supervised_channel_owner().unwrap();
        assert_eq!(register(&mut w, &s, &service), service_id);
        let cutoff = std::time::UNIX_EPOCH + Duration::from_millis(expires + 1);
        std::thread::sleep(
            cutoff
                .duration_since(std::time::SystemTime::now())
                .unwrap_or_default(),
        );
        // An expired account cannot send through either original approved endpoint.
        for (i, (_, endpoint)) in accounts.iter().enumerate() {
            assert!(
                w.start_http(resource_request(&s, endpoint, 51 + i as u8))
                    .is_err()
            );
        }
        assert_eq!(service.data.lock().unwrap().resource_accounts.len(), 2);
        for (i, ((reference, revision, _), endpoint)) in accounts.iter().enumerate() {
            let nonce = 60 + i as u8;
            let refresh = call(&mut w, &s, Action::Refresh, |mut p| {
                p.set_service(&service_id);
                p.set_account(reference);
                p.set_key(&[nonce; 32]);
            });
            assert_eq!(refresh.phase, 1, "{}", refresh.error);
            let done = completed(&mut w, &s, service_id, nonce);
            assert_eq!(done.accounts, vec![(*reference, revision + 2, 1)]);
            assert_eq!(
                query(&mut w, &s, service_id, nonce, Action::Poll).accounts,
                done.accounts
            );
            resource_call(&mut w, &s, endpoint, 70 + i as u8, ["alpha", "beta"][i]);
        }
        {
            let observed = service.data.lock().unwrap();
            assert_eq!((observed.exchanges, observed.rotations), (2, 2));
        }
        let ((ref_a, revision_a, _), ep_a) = &accounts[0];
        let out = call(&mut w, &s, Action::Logout, |mut p| {
            p.set_service(&service_id);
            p.set_account(ref_a);
            p.set_expected_revision(revision_a + 2);
        });
        assert_eq!(out.accounts[0].2, 4);
        assert!(w.start_http(resource_request(&s, ep_a, 45)).is_err());
        resource_call(&mut w, &s, &accounts[1].1, 46, "beta");
        assert_eq!(
            service.data.lock().unwrap().resource_accounts,
            vec![
                Some("alpha".into()),
                Some("beta".into()),
                Some("alpha".into()),
                Some("beta".into()),
                Some("beta".into())
            ]
        );
        w.finish().unwrap();
        server.abort();
    }
}
