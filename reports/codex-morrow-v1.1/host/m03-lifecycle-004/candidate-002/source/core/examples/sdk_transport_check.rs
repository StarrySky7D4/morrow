use morrow_core::response::{Failure, Outcome, Response};
fn main() {
    let path = std::path::PathBuf::from(std::env::args().nth(1).expect("reply directory"));
    let request = morrow_core::runtime::RenameRequest::decode(
        &std::fs::read(path.join("request.capnp")).unwrap(),
    )
    .unwrap();
    assert_eq!(request.operation_id, "vector-op");
    assert_eq!(request.card_id, "legacy-123");
    assert_eq!(request.expected_revision, 1);
    assert_eq!(request.title, "SDK typed rename");
    let denied = Response::decode(&std::fs::read(path.join("denied.capnp")).unwrap()).unwrap();
    assert_eq!(denied.request_id, "vector-op");
    assert_eq!(denied.outcome, Outcome::Rejected(Failure::Denied));
    let committed =
        Response::decode(&std::fs::read(path.join("committed.capnp")).unwrap()).unwrap();
    assert_eq!(committed.request_id, "vector-op");
    let Outcome::Renamed(receipt) = committed.outcome else {
        panic!("expected committed rename")
    };
    assert_eq!(receipt.card_id, "legacy-123");
    assert_eq!(receipt.operation_id, "vector-op");
    assert_eq!(receipt.revision, 2);
    println!(
        "PASS: independently decoded SDK replies match authorization, correlation and revision"
    );
}
