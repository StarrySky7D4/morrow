use morrow_core::{
    io::{Kind, Request, Response, Status},
    io_broker::{ApprovedIo, IoBroker, IoIdentity, SelectedFile},
};
use std::collections::BTreeSet;

fn identity(generation: u64) -> IoIdentity {
    IoIdentity {
        host: 1,
        connection: 7,
        generation,
        package_digest: [3; 32],
    }
}

fn approval() -> ApprovedIo {
    ApprovedIo::from_manager(BTreeSet::from([Kind::FileRead]))
}

fn read_all(broker: &mut IoBroker, identity: &IoIdentity, token: &[u8; 32], file: &[u8]) {
    let mut offset = 0u64;
    let mut out = Vec::new();
    loop {
        let request = Request::encode("read", token, Kind::FileRead, offset, 0, "").unwrap();
        let bytes = broker
            .exchange(identity, &approval(), request.bytes(), 10, false)
            .unwrap();
        let response = Response::decode(&request, &bytes).unwrap();
        assert_eq!(response.status, Status::Ok);
        out.extend_from_slice(&response.payload);
        offset += response.payload.len() as u64;
        if response.eof {
            break;
        }
    }
    assert_eq!(out, file);
}

#[test]
fn selected_file_is_read_in_host_fixed_chunks() {
    let mut broker = IoBroker::new([9; 32]);
    let file = (0..80_000u32).map(|n| (n % 251) as u8).collect::<Vec<_>>();
    let grant = broker
        .grant_file(
            identity(1),
            &approval(),
            SelectedFile::from_fixed_bytes(file.clone()).unwrap(),
            100,
            1,
        )
        .unwrap();
    read_all(&mut broker, &identity(1), grant.as_bytes(), &file);
}

#[test]
fn guessed_or_foreign_refs_are_denied() {
    let mut broker = IoBroker::new([9; 32]);
    let grant = broker
        .grant_file(
            identity(1),
            &approval(),
            SelectedFile::from_fixed_bytes(b"abc".to_vec()).unwrap(),
            100,
            1,
        )
        .unwrap();
    let guess = Request::encode("g", &[0u8; 32], Kind::FileRead, 0, 3, "").unwrap();
    let denied = broker
        .exchange(&identity(1), &approval(), guess.bytes(), 10, false)
        .unwrap();
    assert_eq!(
        Response::decode(&guess, &denied).unwrap().status,
        Status::Revoked
    );
    let other = identity(1);
    let mut other = other;
    other.connection = 8;
    let steal = Request::encode("s", grant.as_bytes(), Kind::FileRead, 0, 3, "").unwrap();
    let cross = broker
        .exchange(&other, &approval(), steal.bytes(), 10, false)
        .unwrap();
    assert_eq!(
        Response::decode(&steal, &cross).unwrap().status,
        Status::Denied
    );
}

#[test]
fn revoke_and_path_and_write_kinds_fail_closed() {
    let mut broker = IoBroker::new([1; 32]);
    let grant = broker
        .grant_file(
            identity(2),
            &approval(),
            SelectedFile::from_fixed_bytes(b"xyz".to_vec()).unwrap(),
            100,
            1,
        )
        .unwrap();
    broker.revoke(&identity(2));
    let request = Request::encode("r", grant.as_bytes(), Kind::FileRead, 0, 3, "").unwrap();
    let revoked = broker
        .exchange(&identity(2), &approval(), request.bytes(), 10, false)
        .unwrap();
    assert_eq!(
        Response::decode(&request, &revoked).unwrap().status,
        Status::Revoked
    );
    let path = Request::encode("p", grant.as_bytes(), Kind::FileRead, 0, 3, "../x").unwrap();
    assert!(matches!(
        broker.exchange(&identity(2), &approval(), path.bytes(), 10, false),
        Err(_)
    ));
    let write = Request::encode("w", grant.as_bytes(), Kind::FileCreate, 0, 0, "").unwrap();
    let unsupported = broker
        .exchange(&identity(2), &approval(), write.bytes(), 10, false)
        .unwrap();
    assert_eq!(
        Response::decode(&write, &unsupported).unwrap().status,
        Status::Revoked
    );
}

#[test]
fn cancel_and_offset_overflow_do_not_invent_eof_success() {
    let mut broker = IoBroker::new([2; 32]);
    let grant = broker
        .grant_file(
            identity(3),
            &approval(),
            SelectedFile::from_fixed_bytes(b"data".to_vec()).unwrap(),
            100,
            1,
        )
        .unwrap();
    let request = Request::encode("c", grant.as_bytes(), Kind::FileRead, 0, 4, "").unwrap();
    let cancelled = broker
        .exchange(&identity(3), &approval(), request.bytes(), 10, true)
        .unwrap();
    assert_eq!(
        Response::decode(&request, &cancelled).unwrap().status,
        Status::Cancelled
    );
    let overflow = Request::encode("o", grant.as_bytes(), Kind::FileRead, 99, 1, "").unwrap();
    let quota = broker
        .exchange(&identity(3), &approval(), overflow.bytes(), 10, false)
        .unwrap();
    assert_eq!(
        Response::decode(&overflow, &quota).unwrap().status,
        Status::Quota
    );
}
