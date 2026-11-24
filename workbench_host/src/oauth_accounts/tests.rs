use super::{
    accounts::{Account, Enrollment, Refresh, State, Update},
    *,
};
use crate::Workbench;
use axum::{
    Json, Router,
    extract::{Form, Query, State as ServerState},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Redirect},
    routing::{get, post},
};
use morrow_core::store::Store;
use serde_json::json;
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};
use tokio::task::JoinHandle;
#[path = "../../tests/support/outbound_plugin.rs"]
mod test_plugin;
#[path = "workbench_tests.rs"]
mod workbench_tests;

#[derive(Default)]
struct Data {
    codes: HashMap<String, (String, String, String)>,
    refresh: HashMap<String, String>,
    access: HashMap<String, String>,
    resource_accounts: Vec<Option<String>>,
    rotations: usize,
    exchanges: usize,
}
#[derive(Clone)]
struct Service {
    origin: String,
    access_seconds: u32,
    data: Arc<Mutex<Data>>,
    arrived: Arc<tokio::sync::Notify>,
    release: Arc<tokio::sync::Notify>,
}
async fn authorize(
    ServerState(service): ServerState<Service>,
    headers: HeaderMap,
    Query(fields): Query<HashMap<String, String>>,
) -> axum::response::Response {
    let account = headers
        .get("x-test-account")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    let Some(redirect) = fields.get("redirect_uri") else {
        return StatusCode::BAD_REQUEST.into_response();
    };
    if !["alpha", "beta", "gamma"].contains(&account)
        || fields.get("client_id").map(String::as_str) != Some("morrow-windows-test")
        || fields.get("code_challenge_method").map(String::as_str) != Some("S256")
        || fields.get("response_type").map(String::as_str) != Some("code")
        || fields.get("scope").map(String::as_str) != Some("cards.read")
    {
        return StatusCode::BAD_REQUEST.into_response();
    }
    let mut data = service.data.lock().unwrap();
    let code = format!("one-time-{account}");
    data.codes.insert(
        code.clone(),
        (
            account.to_owned(),
            redirect.clone(),
            fields["code_challenge"].clone(),
        ),
    );
    let mut target = Url::parse(redirect).unwrap();
    target
        .query_pairs_mut()
        .append_pair("code", &code)
        .append_pair("iss", &service.origin)
        .append_pair("state", &fields["state"]);
    Redirect::temporary(target.as_str()).into_response()
}
async fn token(
    ServerState(service): ServerState<Service>,
    Form(form): Form<HashMap<String, String>>,
) -> axum::response::Response {
    let denied = || {
        (
            StatusCode::BAD_REQUEST,
            Json(json!({"error":"invalid_grant"})),
        )
            .into_response()
    };
    if form.get("client_id").map(String::as_str) != Some("morrow-windows-test") {
        return denied();
    }
    let mut d = service.data.lock().unwrap();
    let Some((account, generation)) = (match form.get("grant_type").map(String::as_str) {
        Some("authorization_code") => form
            .get("code")
            .and_then(|code| d.codes.remove(code))
            .and_then(|(account, redirect, challenge)| {
                let verified = form.get("code_verifier").is_some_and(|v| {
                    v.len() == 43
                        && URL_SAFE_NO_PAD.encode(Sha256::digest(v.as_bytes())) == challenge
                });
                if !verified || form.get("redirect_uri") != Some(&redirect) {
                    return None;
                }
                d.exchanges += 1;
                Some((account, 0))
            }),
        Some("refresh_token") => form
            .get("refresh_token")
            .and_then(|v| d.refresh.remove(v))
            .map(|account| {
                d.rotations += 1;
                (account, d.rotations)
            }),
        _ => None,
    }) else {
        return denied();
    };
    let refresh = format!("fixture-refresh-{account}-{generation}");
    d.refresh.insert(refresh.clone(), account.clone());
    let access = format!("fixture-access-{account}-{generation}");
    d.access.retain(|_, owner| owner != &account);
    d.access.insert(access.clone(), account);
    (StatusCode::OK,Json(json!({"access_token":access,"refresh_token":refresh,"token_type":"Bearer","expires_in":service.access_seconds,"scope":"cards.read"}))).into_response()
}
async fn resource(
    ServerState(service): ServerState<Service>,
    headers: HeaderMap,
) -> axum::response::Response {
    let mut data = service.data.lock().unwrap();
    let account = headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .and_then(|token| data.access.get(token))
        .cloned();
    data.resource_accounts.push(account.clone());
    match account {
        Some(name) => (StatusCode::OK, name).into_response(),
        None => StatusCode::UNAUTHORIZED.into_response(),
    }
}
async fn service() -> (Service, JoinHandle<()>) {
    service_with_lifetime(3).await
}
async fn delayed_resource(
    ServerState(service): ServerState<Service>,
    headers: HeaderMap,
) -> axum::response::Response {
    let response = resource(ServerState(service.clone()), headers).await;
    service.arrived.notify_one();
    match tokio::time::timeout(
        std::time::Duration::from_secs(8),
        service.release.notified(),
    )
    .await
    {
        Ok(()) => response,
        Err(_) => StatusCode::GATEWAY_TIMEOUT.into_response(),
    }
}
async fn service_with_lifetime(access_seconds: u32) -> (Service, JoinHandle<()>) {
    let socket = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let service = Service {
        access_seconds,
        origin: format!("http://{}", socket.local_addr().unwrap()),
        data: Default::default(),
        arrived: Default::default(),
        release: Default::default(),
    };
    let routes = Router::new()
        .route("/authorize", get(authorize))
        .route("/token", post(token))
        .route("/resource", get(resource))
        .route("/waiting", get(delayed_resource))
        .with_state(service.clone());
    (
        service,
        tokio::spawn(async move { axum::serve(socket, routes).await.unwrap() }),
    )
}
async fn browse(url: &Url, account: Option<&str>) -> morrow_network_node::HttpResponse {
    let client = Client::new(
        EndpointPolicy::new(&url.origin().ascii_serialization(), &["GET"], true).unwrap(),
        Limits::default(),
    )
    .unwrap();
    client
        .send(
            HttpRequest {
                method: "GET".into(),
                target: url.as_str().into(),
                headers: account
                    .into_iter()
                    .map(|v| ("x-test-account".to_owned(), v.to_owned()))
                    .collect(),
                body: vec![],
            },
            CancellationToken::new(),
        )
        .await
        .unwrap()
}
async fn connect(store: &mut Store, provider: &Provider, account_name: &str, now: u64) -> Account {
    let e = Enrollment::new(store, provider.clone(), [7; 32], now, 60000)
        .await
        .unwrap();
    let account = e.account.clone();
    let response = browse(&e.browser_url(), Some(account_name)).await;
    assert_eq!(response.status, 307);
    let redirect = Url::parse(
        &response
            .headers
            .iter()
            .find(|(name, _)| name == "location")
            .unwrap()
            .1,
    )
    .unwrap();
    let outcome = tokio::spawn(e.receive(CancellationToken::new()));
    for field in ["state", "iss"] {
        let mut bad = redirect.clone();
        let fields: Vec<_> = bad
            .query_pairs()
            .map(|(k, v)| (k.into_owned(), v.into_owned()))
            .collect();
        bad.set_query(None);
        for (k, v) in fields {
            bad.query_pairs_mut()
                .append_pair(&k, if k == field { "wrong" } else { &v });
        }
        assert_eq!(browse(&bad, None).await.status, 400);
    }
    let mut duplicate = redirect.clone();
    duplicate.query_pairs_mut().append_pair("state", "another");
    assert_eq!(browse(&duplicate, None).await.status, 400);
    assert_eq!(browse(&redirect, None).await.status, 200);
    let update = outcome.await.unwrap().unwrap();
    assert_eq!(update.commit(store, now).unwrap().state, State::Ready);
    account
}
fn begin(account: &Account, store: &mut Store, now: u64) -> accounts::Rotation {
    match account.begin_refresh(store, now).unwrap() {
        Refresh::Start(r) => r,
        _ => panic!("expected one admitted rotation"),
    }
}
fn auth_header(store: &Store, account: &Account, provider: &Provider, now: u64) -> String {
    let record = store
        .load_outbound_authority(&account.reference())
        .unwrap()
        .unwrap();
    let scope = morrow_audit::oauth_credentials::resource_scope(
        &store.tls_store_identity().unwrap(),
        &[7; 32],
        &provider.resource.origin().ascii_serialization(),
    );
    let (expected, expiry) = morrow_audit::oauth_credentials::access_policy(&record).unwrap();
    assert_eq!(expected, scope);
    assert!(now < expiry);
    morrow_audit::oauth_credentials::bearer(scope, &record)
        .unwrap()
        .to_string()
}
#[tokio::test]
async fn two_accounts_reopen_single_rotation_wrong_issuer_and_signout_wins() {
    let (service, server) = service().await;
    let provider = Provider::fixture(&service.origin);
    assert!(
        Provider::reviewed(
            &service.origin,
            &format!("{}/authorize", service.origin),
            &format!("{}/token", service.origin),
            &service.origin,
            "test",
            "cards.read"
        )
        .is_err()
    );
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("oauth.db");
    let mut store = Store::open(&db, Default::default()).unwrap();
    let now = 123_000;
    let alice = connect(&mut store, &provider, "alpha", now).await;
    let bob = connect(&mut store, &provider, "beta", now).await;
    assert_ne!(alice.reference(), bob.reference());
    assert_eq!(
        auth_header(&store, &alice, &provider, now),
        "Bearer fixture-access-alpha-0"
    );
    assert_eq!(
        auth_header(&store, &bob, &provider, now),
        "Bearer fixture-access-beta-0"
    );
    assert_eq!(service.data.lock().unwrap().exchanges, 2);
    drop(store);
    let mut store = Store::open_existing(&db, Default::default()).unwrap();
    assert!(matches!(
        alice.begin_refresh(&mut store, now).unwrap(),
        Refresh::Current(_)
    ));
    let wrong = Account::bind(provider.clone(), [8; 32], alice.reference()).unwrap();
    assert_eq!(wrong.status(&store, now).unwrap_err(), Error::NotApproved);
    let alice_record = store
        .load_outbound_authority(&alice.reference())
        .unwrap()
        .unwrap();
    assert!(morrow_audit::credentials::open(&alice_record).is_err());
    assert!(morrow_audit::oauth_credentials::bearer([9; 32], &alice_record).is_err());
    assert!(!alice_record.container().windows(7).any(|v| v == b"fixture"));
    let rotate = begin(&alice, &mut store, now + 3001);
    assert!(matches!(
        alice.begin_refresh(&mut store, now + 3001).unwrap(),
        Refresh::Pending(_)
    ));
    let mut concurrent = Store::open_existing(&db, Default::default()).unwrap();
    assert!(matches!(
        alice.begin_refresh(&mut concurrent, now + 3001).unwrap(),
        Refresh::Pending(_)
    ));
    let next = rotate.exchange(now + 3001, CancellationToken::new()).await;
    assert_eq!(next.commit(&mut store, now + 3002).unwrap().revision, 3);
    assert!(matches!(
        alice.begin_refresh(&mut concurrent, now + 3002).unwrap(),
        Refresh::Current(_)
    ));
    assert_eq!(
        auth_header(&store, &alice, &provider, now + 3002),
        "Bearer fixture-access-alpha-1"
    );
    assert_eq!(bob.status(&store, now + 3002).unwrap().revision, 1);
    assert_eq!(service.data.lock().unwrap().rotations, 1);
    let rotation = begin(&bob, &mut store, now + 3002);
    let late: Update = rotation
        .exchange(now + 3002, CancellationToken::new())
        .await;
    let signed_out = bob.sign_out(&mut concurrent, 2, now + 3003).unwrap();
    assert_eq!(signed_out.state, State::SignedOut);
    assert_eq!(
        late.commit(&mut store, now + 3003).unwrap_err(),
        Error::NotApproved
    );
    assert!(matches!(
        bob.begin_refresh(&mut store, now + 3004),
        Err(Error::Reauthorize)
    ));
    assert_eq!(
        alice.status(&store, now + 3004).unwrap().state,
        State::Ready
    );
    service.data.lock().unwrap().refresh.clear();
    let invalid = begin(&alice, &mut store, now + 7000)
        .exchange(now + 7000, CancellationToken::new())
        .await;
    assert_eq!(
        invalid.commit(&mut store, now + 7000).unwrap_err(),
        Error::Reauthorize
    );
    assert_eq!(
        alice.status(&store, now + 7000).unwrap().state,
        State::SignInRequired
    );
    assert_eq!(service.data.lock().unwrap().rotations, 2);
    server.abort();
}
