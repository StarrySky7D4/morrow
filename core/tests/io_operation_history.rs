//! Synthetic HTTP history and public fixed-schema query qualification only.
//! These tests create no socket, credential, protected owner or backend dispatch.
#![cfg(not(target_arch = "wasm32"))]
use capnp::{message::Builder, serialize};
use morrow_core::{
    Error, Result,
    io::{self, Action, Header, HttpOutcome, HttpSubmission, OperationOutcome, Request, Response, Status},
    io_capnp as wire,
    io_evidence::{Kind, Material},
    io_intent::{Command, ObservationSource, Record},
    plugin_package::io::IoCapability,
    store::{EventBudget, Store},
};
use sha2::{Digest, Sha256};
use std::{fs, path::PathBuf};

const SUBJECT: &str = "plugin.http-history";
struct Fixture {
    store: Store,
    path: PathBuf,
    // Store must close before TempDir, including during panic unwinding.
    _temp: tempfile::TempDir,
}
impl Fixture {
    fn new() -> Self {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("history.db");
        let store = Store::open(&path, EventBudget::default()).unwrap();
        Self { store, path, _temp: temp }
    }
    fn mutate(&self, statement: &str) {
        let connection = rusqlite::Connection::open(&self.path).unwrap();
        connection.execute_batch(statement).unwrap();
    }
    fn bytes(&self) -> Vec<(String, Option<[u8; 32]>)> {
        // SQLite lock/read-mark bytes in -shm are engine owned. The durable DB
        // and WAL plus the logical writer namespaces must remain unchanged.
        [self.path.clone(), self.path.with_extension("db-wal")].into_iter()
            .map(|path| (path.file_name().unwrap().to_string_lossy().into_owned(),
                fs::read(path).ok().map(|bytes| Sha256::digest(bytes).into())))
            .collect()
    }
    fn counts(&self) -> Vec<i64> {
        let connection = rusqlite::Connection::open(&self.path).unwrap();
        ["operations", "operation_events", "outbox", "io_intents", "io_reservations", "io_evidence", "io_material_reservations"]
            .into_iter().map(|table| connection.query_row(&format!("SELECT count(*) FROM {table}"), [], |row| row.get(0)).unwrap()).collect()
    }
    fn lookup(&self, expected: &Command) -> Result<Option<OperationOutcome>> {
        let bytes = self.bytes(); let counts = self.counts();
        let result = self.store.lookup_http_operation_status(expected);
        assert_eq!(self.counts(), counts, "query must not mutate writer namespaces");
        assert_eq!(self.bytes(), bytes, "query must not change durable DB/WAL bytes");
        result
    }
}
fn submit(operation: &str, call: u64) -> Request {
    Request::encode_http_submit(call, &HttpSubmission {
        operation_id: operation.as_bytes().to_vec(), deadline_ms: 1000,
        endpoint: b"synthetic-endpoint".to_vec(), method: "POST".into(),
        relative_target: "/synthetic".into(), headers: Vec::new(),
        body: b"synthetic-request".to_vec(), credential: Vec::new(),
    }).unwrap()
}
fn command(operation: &str, request: &Request) -> Command {
    Command {
        operation_id: operation.into(), subject: SUBJECT.into(),
        package_sha256: [1; 32], capability: IoCapability::HttpRequest,
        protocol_sha256: io::schema_digest(), request_sha256: request.digest(),
        approval_sha256: [2; 32], target_sha256: [3; 32],
        request_bytes: request.bytes().len() as u64, response_limit: 4096,
    }
}
fn append(f: &mut Fixture, record: &Record) {
    f.store.append_io_intent_local_authorized(record, || Ok(())).unwrap();
}
fn prepared(f: &mut Fixture, expected: &Command) -> Record {
    let record = Record::prepared(expected.clone()).unwrap(); append(f, &record); record
}
fn unknown(f: &mut Fixture, expected: &Command, request: &Request) -> Record {
    let initial = prepared(f, expected);
    f.store.reserve_io_materials(expected, || Ok(())).unwrap();
    let original = Material::encode(Kind::Request, &expected.operation_id, SUBJECT,
        expected.request_sha256, request.bytes()).unwrap();
    f.store.store_io_material(SUBJECT, Kind::Request, &original, || Ok(())).unwrap();
    f.store.reserve_io_intent_followup(expected, || Ok(())).unwrap();
    let record = initial.propose_dispatch_boundary().unwrap();
    f.store.claim_io_dispatch_local_authorized(&record, || Ok(())).unwrap();
    record
}
fn observe_bytes(f: &mut Fixture, expected: &Command, request: &Request, response: &[u8], source: ObservationSource, digest: Option<[u8; 32]>) {
    let before = unknown(f, expected, request);
    let original = Material::encode(Kind::Response, &expected.operation_id, SUBJECT,
        expected.request_sha256, response).unwrap();
    f.store.store_io_material(SUBJECT, Kind::Response, &original, || Ok(())).unwrap();
    let record = before.propose_observation(digest.unwrap_or_else(|| original.payload_sha256()), source).unwrap();
    append(f, &record);
}
fn observed(f: &mut Fixture, operation: &str, status: Status, http_status: u16) -> Command {
    let request = submit(operation, 7); let expected = command(operation, &request);
    let response = Response::encode_http(&request, &HttpOutcome {
        status, http_status,
        headers: if status == Status::Completed { vec![Header { name: "set-cookie".into(), value: b"synthetic-private-header".to_vec() }] } else { Vec::new() },
        body: if status == Status::Completed { b"synthetic-private-body".to_vec() } else { Vec::new() },
    }).unwrap();
    observe_bytes(f, &expected, &request, &response, ObservationSource::OriginalResponse, None);
    expected
}
fn query_frame(operation: &[u8]) -> Vec<u8> {
    let mut message = Builder::new_default(); let mut root = message.init_root::<wire::request::Builder>();
    root.set_version(io::VERSION); root.set_schema_sha256(&io::schema_digest());
    root.set_call_id(19); root.set_query_operation(operation);
    serialize::write_message_to_words(&message)
}
fn query_reply(query: &Request, modify: impl FnOnce(wire::response::Builder<'_>)) -> Vec<u8> {
    let mut message = Builder::new_default(); let mut root = message.init_root::<wire::response::Builder>();
    root.set_version(io::VERSION); root.set_schema_sha256(&io::schema_digest());
    root.set_call_id(query.call_id()); root.set_request_sha256(&query.digest());
    root.set_status(Status::Completed); root.set_http_status(200); root.set_eof(true);
    modify(root); serialize::write_message_to_words(&message)
}

#[test]
fn query_union_is_typed_and_preserves_exact_bytes_digest_and_utf8_identity() {
    for operation in [b"operation".to_vec(), "合法操作".as_bytes().to_vec(), vec![b'x'; io::MAX_OPERATION_BYTES]] {
        let bytes = query_frame(&operation); let request = Request::decode(&bytes).unwrap();
        assert_eq!(request.action(), &Action::QueryOperation { operation_id: operation.clone() });
        assert_eq!(request.bytes(), bytes); assert_eq!(request.digest(), <[u8; 32]>::from(Sha256::digest(&bytes)));
        let generated = Request::query_operation(u64::MAX, &operation).unwrap();
        assert_eq!(generated.call_id(), u64::MAX);
        assert_eq!(generated.action(), &Action::QueryOperation { operation_id: operation });
    }
}

#[test]
fn query_identity_rejects_empty_oversized_invalid_utf8_and_store_invalid_characters() {
    for operation in [Vec::new(), vec![b'x'; io::MAX_OPERATION_BYTES + 1], vec![0xff], b"x/y".to_vec(), b"x\\y".to_vec(), b"x:y".to_vec(), b"x\0y".to_vec(), b"x\ny".to_vec()] {
        assert!(Request::query_operation(1, &operation).is_err());
        assert!(Request::decode(&query_frame(&operation)).is_err());
    }
}

#[test]
fn all_legal_status_and_call_id_extremes_fit_fixed_query_response_reservation() {
    for call in [0, u64::MAX] {
        let query = Request::query_operation(call, b"operation").unwrap();
        for status in [Status::Accepted, Status::Pending, Status::Completed, Status::Denied, Status::Revoked, Status::Expired, Status::Unsupported, Status::Quota, Status::NotFound, Status::Conflict, Status::Cancelled, Status::OutcomeUnknown, Status::EvidenceUnavailable, Status::Failed] {
            let http_values: &[u16] = if status == Status::Completed { &[100, 200, 404, 500, 599] } else { &[0] };
            for &http_status in http_values {
                let expected = OperationOutcome { status, http_status };
                let bytes = Response::encode_operation(&query, &expected).unwrap();
                assert!(bytes.len() <= io::MAX_OPERATION_RESPONSE_BYTES);
                assert_eq!(Response::decode_operation(&query, &bytes).unwrap(), expected);
                let generic = Response::decode(&query, &bytes).unwrap();
                assert_eq!(generic.status, status); assert!(generic.payload.is_empty());
                assert_eq!(generic.offset, 0); assert!(generic.eof);
            }
        }
        for (status, http_status) in [(Status::Invalid, 0), (Status::Completed, 0), (Status::Completed, 99), (Status::Completed, 600), (Status::Completed, u16::MAX), (Status::Pending, 100), (Status::Failed, u16::MAX)] {
            assert!(Response::encode_operation(&query, &OperationOutcome { status, http_status }).is_err());
            let bytes = query_reply(&query, |mut root| { root.set_status(status); root.set_http_status(http_status); });
            assert!(Response::decode_operation(&query, &bytes).is_err());
            assert!(Response::decode(&query, &bytes).is_err());
        }
    }
}

#[test]
fn query_projection_rejects_original_payload_fields_and_wrong_correlation() {
    let query = Request::query_operation(99, b"operation").unwrap();
    for which in 0..8 {
        let bytes = query_reply(&query, |mut root| match which {
            0 => root.set_reference(b"reference"), 1 => root.set_bytes(b"body"),
            2 => { let mut header = root.init_headers(1).get(0); header.set_name("a"); header.set_value(b"b"); },
            3 => root.set_offset(1), 4 => root.set_eof(false),
            5 => root.set_call_id(100), 6 => root.set_request_sha256(&[9; 32]),
            7 => root.set_version(io::VERSION + 1), _ => unreachable!(),
        });
        assert!(Response::decode_operation(&query, &bytes).is_err());
        assert!(Response::decode(&query, &bytes).is_err());
    }
    let huge = query_reply(&query, |mut root| root.set_bytes(&[0; 257]));
    assert_eq!(Response::decode_operation(&query, &huge), Err(Error::Limit));
}

#[test]
fn operation_helpers_reject_non_query_and_old_generic_success_rules_remain_strict() {
    let read = Request::encode_read(7, &[4; 32], 0, 3).unwrap();
    let submit = submit("operation", 7);
    let outcome = OperationOutcome { status: Status::Completed, http_status: 200 };
    for other in [&read, &submit] {
        assert!(Response::encode_operation(other, &outcome).is_err());
        let query = Request::query_operation(7, b"operation").unwrap();
        assert!(Response::decode_operation(other, &Response::encode_operation(&query, &outcome).unwrap()).is_err());
        let bad = query_reply(other, |_| {});
        assert!(Response::decode(other, &bad).is_err());
    }
    let query = Request::query_operation(7, b"operation").unwrap();
    assert!(Response::encode(&query, Status::Completed, b"", 0, true).is_err());
    assert!(Response::encode(&read, Status::Completed, b"abcd", 0, true).is_err());
}

#[test]
fn missing_prepared_unknown_and_cancelled_are_read_only_and_never_dispatch() {
    let mut f = Fixture::new(); let request = submit("missing", 7); let missing = command("missing", &request);
    assert_eq!(f.lookup(&missing).unwrap(), None);
    let request = submit("prepared", 7); let expected = command("prepared", &request);
    prepared(&mut f, &expected);
    for _ in 0..2 { assert_eq!(f.lookup(&expected).unwrap(), Some(OperationOutcome { status: Status::Pending, http_status: 0 })); }
    let request = submit("unknown", 7); let expected = command("unknown", &request);
    unknown(&mut f, &expected, &request);
    for _ in 0..2 { assert_eq!(f.lookup(&expected).unwrap(), Some(OperationOutcome { status: Status::OutcomeUnknown, http_status: 0 })); }
    let request = submit("cancelled", 7); let expected = command("cancelled", &request);
    let initial = prepared(&mut f, &expected); append(&mut f, &initial.propose_cancel_before_dispatch().unwrap());
    assert_eq!(f.lookup(&expected).unwrap(), Some(OperationOutcome { status: Status::Cancelled, http_status: 0 }));
}

#[test]
fn observed_returns_actual_http_and_noncompleted_status_without_secret_material() {
    for (status, http_status) in [(Status::Completed, 100), (Status::Completed, 200), (Status::Completed, 404), (Status::Completed, 500), (Status::Completed, 599), (Status::Failed, 0), (Status::OutcomeUnknown, 0), (Status::Cancelled, 0)] {
        let mut f = Fixture::new(); let expected = observed(&mut f, "observed", status, http_status);
        for _ in 0..2 { assert_eq!(f.lookup(&expected).unwrap(), Some(OperationOutcome { status, http_status })); }
        let query = Request::query_operation(88, b"observed").unwrap();
        let projection = Response::encode_operation(&query, &f.lookup(&expected).unwrap().unwrap()).unwrap();
        assert!(!projection.windows(b"synthetic-private".len()).any(|w| w == b"synthetic-private"));
    }
}

#[test]
fn unknown_and_observed_survive_reopen_without_query_mutation_or_replay() {
    let mut f = Fixture::new(); let observed = observed(&mut f, "observed", Status::Completed, 503);
    let request = submit("unknown", 7); let pending = command("unknown", &request); unknown(&mut f, &pending, &request);
    // Close the original connection before opening exactly the same synthetic DB.
    let Fixture { store, path, _temp } = f; drop(store);
    let store = Store::open_existing(&path, EventBudget::default()).unwrap();
    let f = Fixture { store, path, _temp };
    assert_eq!(f.lookup(&observed).unwrap(), Some(OperationOutcome { status: Status::Completed, http_status: 503 }));
    assert_eq!(f.lookup(&pending).unwrap(), Some(OperationOutcome { status: Status::OutcomeUnknown, http_status: 0 }));
}

#[test]
fn reconciliation_observation_is_not_a_completed_http_result() {
    let mut f = Fixture::new(); let request = submit("reconciled", 7); let expected = command("reconciled", &request);
    let before = unknown(&mut f, &expected, &request);
    f.store.release_io_material_reconciliation(SUBJECT, "reconciled", Kind::Response).unwrap();
    append(&mut f, &before.propose_observation([7; 32], ObservationSource::Reconciliation).unwrap());
    assert_eq!(f.lookup(&expected), Err(Error::EvidenceUnavailable));
}

#[test]
fn missing_or_corrupt_request_and_response_originals_never_become_completed() {
    for mutation in [
        "DELETE FROM io_evidence WHERE kind=1", "DELETE FROM io_evidence WHERE kind=2",
        "UPDATE io_evidence SET container=X'00' WHERE kind=1",
        "UPDATE io_evidence SET container=X'00' WHERE kind=2",
        "UPDATE io_evidence SET payload_sha256=zeroblob(32) WHERE kind=2",
        "UPDATE io_evidence SET container=zeroblob(300000) WHERE kind=2",
        "UPDATE io_evidence SET subject='foreign' WHERE kind=1",
    ] {
        let mut f = Fixture::new(); let expected = observed(&mut f, "observed", Status::Completed, 200);
        f.mutate(mutation); assert_eq!(f.lookup(&expected), Err(Error::EvidenceUnavailable));
    }
}

#[test]
fn observation_digest_must_match_the_actual_original_response_material() {
    let request = submit("digest", 7); let expected = command("digest", &request);
    let response = Response::encode_http(&request, &HttpOutcome { status: Status::Completed, http_status: 200, headers: Vec::new(), body: Vec::new() }).unwrap();
    let material = Material::encode(Kind::Response, "digest", SUBJECT, expected.request_sha256, &response).unwrap();
    assert_ne!(material.digest(), material.payload_sha256());
    // Production Broker records the exact response payload digest. The valid
    // material container digest is independently checked by storage, but cannot
    // substitute for that observation digest.
    for bad_digest in [[9; 32], material.digest()] {
        let mut f = Fixture::new();
        observe_bytes(&mut f, &expected, &request, &response, ObservationSource::OriginalResponse, Some(bad_digest));
        assert_eq!(f.lookup(&expected), Err(Error::EvidenceUnavailable));
    }
}

#[test]
fn original_response_requires_exact_submit_call_and_request_digest_correlation() {
    for different_call in [8, u64::MAX] {
        let mut f = Fixture::new(); let request = submit("correlation", 7); let expected = command("correlation", &request);
        let other = submit("correlation", different_call);
        let response = Response::encode_http(&other, &HttpOutcome { status: Status::Completed, http_status: 200, headers: Vec::new(), body: Vec::new() }).unwrap();
        observe_bytes(&mut f, &expected, &request, &response, ObservationSource::OriginalResponse, None);
        assert_eq!(f.lookup(&expected), Err(Error::EvidenceUnavailable));
    }
}

#[test]
fn original_request_must_be_http_submit_for_the_exact_operation_identity() {
    for request in [Request::encode_read(7, &[4; 32], 0, 3).unwrap(), submit("another-operation", 7)] {
        let mut f = Fixture::new(); let expected = command("expected-operation", &request);
        let bytes = if matches!(request.action(), Action::SubmitHttp(_)) {
            Response::encode_http(&request, &HttpOutcome { status: Status::Completed, http_status: 200, headers: Vec::new(), body: Vec::new() }).unwrap()
        } else { Response::encode(&request, Status::Completed, b"abc", 0, true).unwrap() };
        observe_bytes(&mut f, &expected, &request, &bytes, ObservationSource::OriginalResponse, None);
        assert_eq!(f.lookup(&expected), Err(Error::EvidenceUnavailable));
    }
}

#[test]
fn foreign_subject_cannot_distinguish_existing_corrupt_history_from_absence() {
    let mut f = Fixture::new(); let expected = observed(&mut f, "owned", Status::Completed, 200);
    let mut foreign = expected.clone(); foreign.subject = "foreign".into();
    assert_eq!(f.lookup(&foreign).unwrap(), None);
    f.mutate("UPDATE operations SET payload=X'00' WHERE object_kind=5");
    assert_eq!(f.lookup(&foreign).unwrap(), None);
    assert_eq!(f.lookup(&expected), Err(Error::EvidenceUnavailable));
    // Only this disposable corruption connection bypasses SQLite FK enforcement.
    // A missing revision-one event removes the proof of either subject's ownership;
    // unlike the corrupt payload above, it must be indistinguishable from absence.
    {
        let connection = rusqlite::Connection::open(&f.path).unwrap();
        connection.pragma_update(None, "foreign_keys", false).unwrap();
        connection.execute_batch("UPDATE io_intents SET event_id='unrelated-missing-initial' WHERE revision=1").unwrap();
        connection.pragma_update(None, "foreign_keys", true).unwrap();
        drop(connection);
    }
    assert_eq!(f.lookup(&foreign).unwrap(), None);
    assert_eq!(f.lookup(&expected).unwrap(), None);
}

#[test]
fn same_subject_mismatched_package_or_command_pins_are_conflict_without_material_reads() {
    let mut f = Fixture::new(); let request = submit("pins", 7); let expected = command("pins", &request); prepared(&mut f, &expected);
    for which in 0..6 {
        let mut wrong = expected.clone();
        match which { 0 => wrong.package_sha256=[9;32], 1 => wrong.request_sha256=[9;32], 2 => wrong.approval_sha256=[9;32], 3 => wrong.target_sha256=[9;32], 4 => wrong.request_bytes+=1, 5 => wrong.response_limit-=1, _ => unreachable!() }
        assert_eq!(f.lookup(&wrong), Err(Error::OperationConflict));
    }
}

#[test]
fn unsupported_capability_protocol_and_oversized_declared_frames_reject_before_lookup() {
    let f = Fixture::new(); let request = submit("not-existing", 7); let expected = command("not-existing", &request);
    let mut wrong = expected.clone(); wrong.capability=IoCapability::FileRead;
    assert_eq!(f.lookup(&wrong), Err(Error::UnsupportedVersion));
    let mut wrong = expected.clone(); wrong.protocol_sha256=[9;32];
    assert_eq!(f.lookup(&wrong), Err(Error::UnsupportedVersion));
    for request_side in [true,false] {
        let mut wrong = expected.clone();
        if request_side { wrong.request_bytes=io::MAX_FRAME_BYTES as u64+1; } else { wrong.response_limit=io::MAX_FRAME_BYTES as u64+1; }
        assert_eq!(f.lookup(&wrong), Err(Error::Limit));
    }
}
