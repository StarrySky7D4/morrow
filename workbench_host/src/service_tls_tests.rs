//! Real TLS admission through the application owner, not just the node factory.
use super::*;
use crate::service_tls::TlsSelection;
use rcgen::{CertifiedKey, generate_simple_self_signed};
use std::{fs, sync::Arc};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio_rustls::{TlsConnector, client::TlsStream};

fn pair(dir: &Path) -> (CertifiedKey, TlsSelection) {
    let certified = generate_simple_self_signed(vec!["localhost".into()]).unwrap();
    let certificate = dir.join("certificate.pem");
    let private_key = dir.join("private-key.pem");
    fs::write(&certificate, certified.cert.pem()).unwrap();
    fs::write(&private_key, certified.key_pair.serialize_pem()).unwrap();
    let selection = TlsSelection::inspect(&certificate, &private_key).unwrap();
    (certified, selection)
}

fn timed_pair(dir: &Path, before: i64, after: i64) -> (CertifiedKey, TlsSelection) {
    let key_pair = rcgen::KeyPair::generate().unwrap();
    let mut params = rcgen::CertificateParams::new(vec!["localhost".into()]).unwrap();
    params.not_before = time::OffsetDateTime::from_unix_timestamp(before).unwrap();
    params.not_after = time::OffsetDateTime::from_unix_timestamp(after).unwrap();
    let cert = params.self_signed(&key_pair).unwrap();
    let certificate = dir.join("certificate.pem");
    let private_key = dir.join("private-key.pem");
    fs::write(&certificate, cert.pem()).unwrap();
    fs::write(&private_key, key_pair.serialize_pem()).unwrap();
    let selection = TlsSelection::inspect(&certificate, &private_key).unwrap();
    (CertifiedKey { cert, key_pair }, selection)
}

#[test]
fn expired_and_future_certificates_are_inspectable_but_cannot_start() {
    let mut fixture = Fixture::new("127.0.0.1:0".parse().unwrap());
    approve_tls(&mut fixture);
    let binding = fixture.app.local_state().unwrap().host.binding();
    let dir = tempfile::tempdir().unwrap();
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;
    for (before, after) in [(now - 100, now - 1), (now + 3600, now + 7200)] {
        let (_, selection) = timed_pair(dir.path(), before, after);
        assert_eq!(selection.validity().unwrap().not_after, after);
        let mut options = fixture.options(1);
        options.publication_revision = 2;
        assert!(
            fixture
                .app
                .start_service_with_network(options, &[], Some(&selection))
                .is_err()
        );
        assert_eq!(fixture.app.local_state().unwrap().host.binding(), binding);
        assert!(fixture.app.io_status().key.is_none());
    }
    let (_, selection) = timed_pair(dir.path(), now - 100, now + 3600);
    let task = start_tls(&mut fixture, &selection, 1);
    running(&mut fixture.app, task);
    stop(&mut fixture, task);
    fixture.app.finish().unwrap();
}

#[test]
fn certificate_expiry_stops_real_tls_listener_and_returns_original_owner() {
    let mut fixture = Fixture::new("127.0.0.1:0".parse().unwrap());
    approve_tls(&mut fixture);
    let binding = fixture.app.local_state().unwrap().host.binding();
    let dir = tempfile::tempdir().unwrap();
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;
    let (cert, selection) = timed_pair(dir.path(), now - 60, now + 5);
    let task = start_tls(&mut fixture, &selection, 1);
    let address = running(&mut fixture.app, task);
    let runtime = tokio::runtime::Runtime::new().unwrap();
    runtime.block_on(async {
        tokio::time::timeout(WAIT, async {
            let mut socket = connect(address, &cert, "localhost").await.unwrap();
            assert!(
                request(&mut socket, TOKEN)
                    .await
                    .ends_with(b"executed-before")
            );
        })
        .await
        .unwrap();
    });
    let result = exited(&mut fixture.app, task);
    assert_eq!(result.listener, Some(Ok(())));
    assert_eq!(fixture.app.local_state().unwrap().host.binding(), binding);
    assert!(TcpStream::connect_timeout(&address, Duration::from_millis(100)).is_err());
    fixture.app.acknowledge_io(task).unwrap();
    let mut options = fixture.options(2);
    options.publication_revision = 2;
    assert!(
        fixture
            .app
            .start_service_with_network(options, &[], Some(&selection))
            .is_err()
    );
    fixture.app.finish().unwrap();
}

fn approve_tls(fixture: &mut Fixture) {
    let state = fixture.app.local_state_mut().unwrap();
    state.host.prepare_write().unwrap();
    let store = state.host.store_local_mut();
    let mut publication = store
        .load_service_authority(&PUBLICATION)
        .unwrap()
        .unwrap()
        .value()
        .clone();
    publication.revision += 1;
    let Some(authority::record::Kind::Publication(value)) = publication.kind.as_mut() else {
        unreachable!()
    };
    value.tls_required = true;
    store
        .save_service_authority_local(&Authority::encode(publication).unwrap(), 1)
        .unwrap();
    state.host.flush_pending().unwrap();
}

fn start_tls(fixture: &mut Fixture, selection: &TlsSelection, submission: u8) -> TaskKey {
    let mut options = fixture.options(submission);
    options.publication_revision = 2;
    fixture
        .app
        .start_service_with_network(options, &[], Some(selection))
        .unwrap()
}

async fn connect(
    address: SocketAddr,
    root: &CertifiedKey,
    name: &'static str,
) -> std::io::Result<TlsStream<tokio::net::TcpStream>> {
    let mut roots = rustls::RootCertStore::empty();
    roots.add(root.cert.der().clone()).unwrap();
    let config = rustls::ClientConfig::builder()
        .with_root_certificates(roots)
        .with_no_client_auth();
    let stream = tokio::net::TcpStream::connect(address).await?;
    TlsConnector::from(Arc::new(config))
        .connect(
            rustls::pki_types::ServerName::try_from(name).unwrap(),
            stream,
        )
        .await
}

async fn request(socket: &mut TlsStream<tokio::net::TcpStream>, token: &str) -> Vec<u8> {
    socket.write_all(format!("POST /api HTTP/1.1\r\nHost: localhost\r\nAuthorization: Bearer {token}\r\nIdempotency-Key: {FIRST_KEY}\r\nContent-Length: 6\r\nConnection: close\r\n\r\nbefore").as_bytes()).await.unwrap();
    let mut output = Vec::new();
    socket.read_to_end(&mut output).await.unwrap();
    output
}

fn stop(fixture: &mut Fixture, task: TaskKey) {
    fixture.app.cancel_io(task).unwrap();
    let stopped = exited(&mut fixture.app, task);
    assert_eq!(stopped.listener, Some(Ok(())));
    let exit = stopped.task.exit.unwrap();
    assert!(exit.execution.is_ok() && exit.disconnect.is_ok() && exit.maintenance.is_ok());
    fixture.app.acknowledge_io(task).unwrap();
}

#[test]
fn tls_serves_authenticated_guest_and_pins_identity_until_explicit_restart() {
    let mut fixture = Fixture::new("127.0.0.1:0".parse().unwrap());
    approve_tls(&mut fixture);
    let binding = fixture.app.local_state().unwrap().host.binding();
    let material = tempfile::tempdir().unwrap();
    let (original, selected) = pair(material.path());
    let task = start_tls(&mut fixture, &selected, 1);
    let address = running(&mut fixture.app, task);
    let before = command(
        &mut fixture.app,
        task,
        frame(wire::Action::ReadUiLocale, |_| {}),
    );
    let saved = command(
        &mut fixture.app,
        task,
        frame(wire::Action::SaveUiLocale, |mut request| {
            request.set_operation("tls-original-owner-locale");
            request.set_revision(before.revision);
            request.set_payload(b"en");
        }),
    );
    assert_eq!(saved.revision, before.revision + 1);
    // Change both files to a valid different identity while the old server runs.
    let (replacement, replacement_selection) = pair(material.path());
    assert!(selected.load().is_err());
    let runtime = tokio::runtime::Runtime::new().unwrap();
    runtime.block_on(async {
        tokio::time::timeout(WAIT, async {
            assert!(connect(address, &original, "wrong.invalid").await.is_err());
            assert!(connect(address, &replacement, "localhost").await.is_err());
            let mut unauthorized = connect(address, &original, "localhost").await.unwrap();
            let output = request(&mut unauthorized, "invalid-token").await;
            assert!(output.starts_with(b"HTTP/1.1 401 "));
            let mut socket = connect(address, &original, "localhost").await.unwrap();
            let output = request(&mut socket, TOKEN).await;
            assert!(output.starts_with(b"HTTP/1.1 202 "));
            assert!(output.ends_with(b"executed-before"));
        })
        .await
        .unwrap();
    });
    stop(&mut fixture, task);
    assert_eq!(fixture.app.local_state().unwrap().host.binding(), binding);
    assert!(TcpStream::connect_timeout(&address, Duration::from_millis(100)).is_err());
    // Explicit new inspection/start accepts the new cert and preserves the
    // original durable request identity: the same request returns its result.
    let task = start_tls(&mut fixture, &replacement_selection, 2);
    let address = running(&mut fixture.app, task);
    runtime.block_on(async {
        tokio::time::timeout(WAIT, async {
            assert!(connect(address, &original, "localhost").await.is_err());
            let mut socket = connect(address, &replacement, "localhost").await.unwrap();
            let output = request(&mut socket, TOKEN).await;
            assert!(output.starts_with(b"HTTP/1.1 202 "));
            assert!(output.ends_with(b"executed-before"));
        })
        .await
        .unwrap();
    });
    stop(&mut fixture, task);
    fixture.app.finish().unwrap();
    drop(fixture.app);
    let mut reopened =
        Workbench::open_managed(fixture.dir.path(), Some(workbench_package())).unwrap();
    let config = reopened
        .local_state()
        .unwrap()
        .host
        .store_local()
        .load_service_config(CONFIG)
        .unwrap()
        .unwrap();
    assert_eq!(config.digest(), fixture.config_digest);
    assert_eq!(config.value().revision, 1);
    assert_eq!(
        reopened.read_ui_locale().unwrap(),
        ("en".into(), saved.revision)
    );
    reopened.finish().unwrap();
}

#[test]
fn tls_mode_and_changed_files_fail_before_owner_move_or_submission_consumption() {
    let mut fixture = Fixture::new("127.0.0.1:0".parse().unwrap());
    let binding = fixture.app.local_state().unwrap().host.binding();
    let material = tempfile::tempdir().unwrap();
    let (_, selected) = pair(material.path());
    let options = fixture.options(1);
    assert!(
        fixture
            .app
            .start_service_with_network(options, &[], Some(&selected))
            .is_err()
    );
    approve_tls(&mut fixture);
    let mut options = fixture.options(1);
    options.publication_revision = 2;
    assert!(fixture.app.start_service(options).is_err());
    let (_, replacement) = pair(material.path());
    let mut options = fixture.options(1);
    options.publication_revision = 2;
    assert!(
        fixture
            .app
            .start_service_with_network(options, &[], Some(&selected))
            .is_err()
    );
    assert_eq!(fixture.app.local_state().unwrap().host.binding(), binding);
    assert!(fixture.app.io_status().key.is_none());
    let task = start_tls(&mut fixture, &replacement, 1);
    running(&mut fixture.app, task);
    stop(&mut fixture, task);
    fixture.app.finish().unwrap();
}

#[test]
fn stopping_with_incomplete_tls_client_reclaims_original_owner() {
    let mut fixture = Fixture::new("127.0.0.1:0".parse().unwrap());
    approve_tls(&mut fixture);
    let binding = fixture.app.local_state().unwrap().host.binding();
    let material = tempfile::tempdir().unwrap();
    let (_, selected) = pair(material.path());
    let task = start_tls(&mut fixture, &selected, 1);
    let address = running(&mut fixture.app, task);
    let mut socket = TcpStream::connect_timeout(&address, WAIT).unwrap();
    socket.set_read_timeout(Some(WAIT)).unwrap();
    socket.set_write_timeout(Some(WAIT)).unwrap();
    // Start a TLS record but never complete its handshake.
    socket
        .write_all(&[0x16, 0x03, 0x03, 0x00, 0x80, 0x01])
        .unwrap();
    stop(&mut fixture, task);
    assert_eq!(fixture.app.local_state().unwrap().host.binding(), binding);
    let mut byte = [0];
    match socket.read(&mut byte) {
        Ok(0) => {}
        Err(error) => assert!(matches!(
            error.kind(),
            std::io::ErrorKind::ConnectionReset | std::io::ErrorKind::ConnectionAborted
        )),
        other => panic!("TLS connection remained live after listener join: {other:?}"),
    }
    assert!(TcpStream::connect_timeout(&address, Duration::from_millis(100)).is_err());
    fixture.app.finish().unwrap();
}
