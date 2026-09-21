use super::*;
use sha2::{Digest, Sha256};

fn stage(app: &mut Workbench, key: TaskKey, token: &str, bytes: &[u8]) {
    let digest = Sha256::digest(bytes);
    command(
        app,
        key,
        frame(wire::Action::CommandFrameBegin, |mut r| {
            r.set_transfer(token);
            r.set_total_length(bytes.len() as u64);
            r.set_sha256(&digest);
        }),
    );
    for (i, part) in bytes.chunks(32768).enumerate() {
        command(
            app,
            key,
            frame(wire::Action::CommandFrameAppend, |mut r| {
                r.set_transfer(token);
                r.set_offset((i * 32768) as u64);
                r.set_payload(part);
            }),
        );
    }
}
fn finish_frame(token: &str, bytes: &[u8]) -> Vec<u8> {
    frame(wire::Action::CommandFrameFinish, |mut r| {
        r.set_transfer(token);
        r.set_total_length(bytes.len() as u64);
        r.set_sha256(&Sha256::digest(bytes));
    })
}
fn rejected(app: &mut Workbench, key: TaskKey, bytes: Vec<u8>) {
    let mut job = app.submit_service_command(key, bytes).unwrap();
    let deadline = Instant::now() + WAIT;
    while job.poll() == OwnerCommandPoll::Pending {
        assert!(Instant::now() < deadline);
        thread::sleep(Duration::from_millis(2));
    }
    let bytes = zeroize::Zeroizing::new(job.read().unwrap().unwrap());
    let mut slice = bytes.as_slice();
    let message =
        capnp::serialize::read_message_from_flat_slice(&mut slice, Default::default()).unwrap();
    assert!(
        !message
            .get_root::<wire::response::Reader>()
            .unwrap()
            .get_error()
            .unwrap()
            .is_empty()
    );
}
#[test]
fn large_complete_business_frame_executes_once_on_original_owner_and_survives_reopen() {
    let mut fixture = Fixture::new("127.0.0.1:0".parse().unwrap());
    let key = fixture.start();
    running(&mut fixture.app, key);
    let before = command(
        &mut fixture.app,
        key,
        frame(wire::Action::ReadUiLocale, |_| {}),
    );
    let bytes = frame(wire::Action::SaveUiLocale, |mut r| {
        r.set_operation("large-locale-once");
        r.set_revision(before.revision);
        r.set_payload(b"en");
        // The complete private request must survive intact, including padding.
        r.set_name("x".repeat(100000).as_str());
    });
    assert!(bytes.len() > 65536 && bytes.len() <= 128 * 1024);
    assert!(
        fixture
            .app
            .submit_service_command(key, bytes.clone())
            .is_err()
    );
    let token = "12".repeat(32);
    stage(&mut fixture.app, key, &token, &bytes);
    let response = command(&mut fixture.app, key, finish_frame(&token, &bytes));
    assert_eq!(response.revision, before.revision + 1);
    rejected(&mut fixture.app, key, finish_frame(&token, &bytes));
    let read = command(
        &mut fixture.app,
        key,
        frame(wire::Action::ReadUiLocale, |_| {}),
    );
    assert_eq!(read.payload, b"en");
    assert_eq!(read.revision, response.revision);
    fixture.app.cancel_io(key).unwrap();
    exited(&mut fixture.app, key);
    assert_eq!(
        fixture
            .app
            .local_state()
            .unwrap()
            .read_ui_locale()
            .unwrap()
            .0,
        "en"
    );
    fixture.app.acknowledge_io(key).unwrap();
    fixture.app.finish().unwrap();
    drop(fixture.app);
    let mut reopened = Workbench::open_managed(fixture.dir.path(), None).unwrap();
    assert_eq!(reopened.read_ui_locale().unwrap().0, "en");
    reopened.finish().unwrap();
}
#[test]
fn staging_cannot_recurse_or_survive_original_worker_exit() {
    let mut fixture = Fixture::new("127.0.0.1:0".parse().unwrap());
    let standalone = protocol::respond(
        &mut fixture.app,
        &frame(wire::Action::CommandFrameBegin, |mut r| {
            r.set_transfer("78".repeat(32).as_str());
            r.set_total_length(1);
            r.set_sha256(&[7; 32]);
        }),
    )
    .unwrap();
    let mut slice = standalone.as_slice();
    let message =
        capnp::serialize::read_message_from_flat_slice(&mut slice, Default::default()).unwrap();
    assert!(
        !message
            .get_root::<wire::response::Reader>()
            .unwrap()
            .get_error()
            .unwrap()
            .is_empty()
    );

    let key = fixture.start();
    running(&mut fixture.app, key);
    for action in [wire::Action::IoCancel, wire::Action::CommandFrameFinish] {
        let bytes = frame(action, |_| {});
        let token = "34".repeat(32);
        stage(&mut fixture.app, key, &token, &bytes);
        rejected(&mut fixture.app, key, finish_frame(&token, &bytes));
    }
    let bytes = frame(wire::Action::ReadUiLocale, |_| {});
    let token = "56".repeat(32);
    stage(&mut fixture.app, key, &token, &bytes);
    fixture.app.cancel_io(key).unwrap();
    exited(&mut fixture.app, key);
    assert!(
        fixture
            .app
            .local_state_mut()
            .unwrap()
            .command_frame
            .take(
                &token,
                bytes.len() as u64,
                &Sha256::digest(&bytes),
                Instant::now()
            )
            .is_err()
    );
    fixture.app.acknowledge_io(key).unwrap();
    fixture.app.finish().unwrap();
}
