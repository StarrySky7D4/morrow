#![cfg(target_os = "windows")]
use morrow_audit::{
    library::{Error, Registry},
    session::{OpenMode, Session},
};
use prost::Message;
use sha2::{Digest, Sha256};

#[derive(Clone, PartialEq, Message)]
struct Selection {
    #[prost(uint32, tag = "1")]
    version: u32,
    #[prost(uint64, tag = "2")]
    generation: u64,
    #[prost(string, tag = "3")]
    directory: String,
    #[prost(string, tag = "4")]
    log_id: String,
    #[prost(bytes = "vec", tag = "5")]
    public_key: Vec<u8>,
}
fn encoded(value: &Selection) -> Vec<u8> {
    let raw = value.encode_to_vec();
    let mut bytes = b"MORROWL1".to_vec();
    bytes.extend_from_slice(&Sha256::digest(&raw));
    bytes.extend(lz4_flex::block::compress_prepend_size(&raw));
    bytes
}
#[test]
fn registered_missing_target_never_recreates_or_falls_back_to_initial_library() {
    let root = tempfile::tempdir().unwrap();
    let candidate = tempfile::tempdir().unwrap();
    drop(
        Session::open(
            &candidate.path().join("workbench.db"),
            Default::default(),
            OpenMode::Initialize,
        )
        .unwrap(),
    );
    let mut registry = Registry::open(root.path()).unwrap();
    drop(registry.open_session(Default::default()).unwrap());
    registry.activate(candidate.path()).unwrap();
    assert_eq!(registry.generation(), 2);
    drop(registry);
    let removed = candidate.path().to_path_buf();
    candidate.close().unwrap();
    let mut registry = Registry::open(root.path()).unwrap();
    assert!(registry.open_session(Default::default()).is_err());
    assert!(!removed.exists());
    assert!(root.path().join("workbench.db").exists());
}
#[test]
fn same_path_replacement_by_another_valid_library_fails_without_mutating_it() {
    let root = tempfile::tempdir().unwrap();
    let mut registry = Registry::open(root.path()).unwrap();
    drop(registry.open_session(Default::default()).unwrap());
    drop(registry);
    let other = tempfile::tempdir().unwrap();
    drop(
        Session::open(
            &other.path().join("workbench.db"),
            Default::default(),
            OpenMode::Initialize,
        )
        .unwrap(),
    );
    for name in ["workbench.db", "workbench.db.audit-key"] {
        std::fs::copy(other.path().join(name), root.path().join(name)).unwrap();
    }
    let before = std::fs::read(root.path().join("workbench.db")).unwrap();
    let mut registry = Registry::open(root.path()).unwrap();
    assert!(matches!(
        registry.open_session(Default::default()),
        Err(Error::IdentityMismatch)
    ));
    assert_eq!(
        std::fs::read(root.path().join("workbench.db")).unwrap(),
        before
    );
}
#[test]
fn malformed_registry_is_bounded_and_never_interpreted_as_first_use() {
    let root = tempfile::tempdir().unwrap();
    let mut valid = Selection {
        version: 1,
        generation: 1,
        directory: root.path().canonicalize().unwrap().to_str().unwrap().into(),
        log_id: "test".into(),
        public_key: vec![0; 32],
    };
    let mut cases = vec![vec![], vec![1; 32769], b"MORROWL1".to_vec()];
    valid.version = 2;
    cases.push(encoded(&valid));
    valid.version = 1;
    valid.generation = 0;
    cases.push(encoded(&valid));
    valid.generation = 1;
    valid.directory = "relative/path".into();
    cases.push(encoded(&valid));
    valid.directory = "C:\\bad\0path".into();
    cases.push(encoded(&valid));
    let mut excessive = b"MORROWL1".to_vec();
    excessive.extend([0; 32]);
    excessive.extend(u32::MAX.to_le_bytes());
    cases.push(excessive);
    for bytes in cases {
        std::fs::write(root.path().join("active-library.pb.lz4"), bytes).unwrap();
        assert!(Registry::open(root.path()).is_err());
        assert!(!root.path().join("workbench.db").exists());
    }
}
#[test]
fn registry_lease_prevents_second_owner_and_releases_without_removing_guard() {
    let root = tempfile::tempdir().unwrap();
    let registry = Registry::open(root.path()).unwrap();
    assert!(matches!(Registry::open(root.path()), Err(Error::Busy)));
    assert!(std::fs::remove_file(root.path().join("active-library.lock")).is_err());
    drop(registry);
    assert!(Registry::open(root.path()).is_ok());
}

#[test]
fn publish_error_requires_new_registry_even_after_filesystem_obstruction_is_removed() {
    use std::os::windows::fs::OpenOptionsExt;
    let root = tempfile::tempdir().unwrap();
    let other = tempfile::tempdir().unwrap();
    drop(
        Session::open(
            &other.path().join("workbench.db"),
            Default::default(),
            OpenMode::Initialize,
        )
        .unwrap(),
    );
    let mut registry = Registry::open(root.path()).unwrap();
    drop(registry.open_session(Default::default()).unwrap());
    let blocker = std::fs::File::options()
        .read(true)
        .share_mode(1)
        .open(root.path().join("active-library.pb.lz4"))
        .unwrap();
    assert!(matches!(
        registry.activate(other.path()),
        Err(Error::PublishUnknown)
    ));
    drop(blocker);
    assert!(matches!(
        registry.selected_database(),
        Err(Error::PublishUnknown)
    ));
    assert!(matches!(
        registry.open_session(Default::default()),
        Err(Error::PublishUnknown)
    ));
    assert!(matches!(
        registry.activate(other.path()),
        Err(Error::PublishUnknown)
    ));
    drop(registry);
    let mut reopened = Registry::open(root.path()).unwrap();
    assert_eq!(reopened.generation(), 1);
    drop(reopened.open_session(Default::default()).unwrap());
    reopened.activate(other.path()).unwrap();
    assert_eq!(reopened.generation(), 2);
}

#[test]
fn expected_identity_is_checked_before_writes_and_never_initializes_missing_database() {
    let root = tempfile::tempdir().unwrap();
    let original = root.path().join("original.db");
    let source = Session::open(&original, Default::default(), OpenMode::Initialize).unwrap();
    let expected = source.trust();
    drop(source);
    let different = root.path().join("different.db");
    drop(Session::open(&different, Default::default(), OpenMode::Initialize).unwrap());
    let db_before = std::fs::read(&different).unwrap();
    let key_path = morrow_audit::session::key_path(&different).unwrap();
    let key_before = std::fs::read(&key_path).unwrap();
    assert!(matches!(
        Session::open_expected(&different, Default::default(), &expected),
        Err(morrow_audit::session::SessionError::KeyMismatch)
    ));
    assert_eq!(std::fs::read(&different).unwrap(), db_before);
    assert_eq!(std::fs::read(&key_path).unwrap(), key_before);
    let missing = root.path().join("missing.db");
    assert!(matches!(
        Session::open_expected(&missing, Default::default(), &expected),
        Err(morrow_audit::session::SessionError::MissingDatabase)
    ));
    assert!(!missing.exists());
    assert!(!morrow_audit::session::key_path(&missing).unwrap().exists());
    assert!(Session::open_expected(&original, Default::default(), &expected).is_ok());
}
