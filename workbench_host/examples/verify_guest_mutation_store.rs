//! Qualification helper for an already-closed, isolated guest content crash test.
//! No production endpoint, guest permission, migration, or replay is introduced.
#[cfg(target_os = "windows")]
fn run() -> Result<(), Box<dyn std::error::Error>> {
    use morrow_audit::{keys::Key, library::Registry, session::key_path};
    use morrow_core::{
        file_content_receipt::Source,
        file_mutation::{Disposition, RequestRecord},
        io_evidence::Kind,
        io_intent::{Phase, Recovery},
        store::Store,
    };
    use sha2::{Digest, Sha256};
    use std::{fs, os::windows::fs::MetadataExt, path::Path};

    fn regular(path: &Path) -> Result<(), Box<dyn std::error::Error>> {
        let metadata = fs::symlink_metadata(path)?;
        if metadata.file_attributes() & 0x400 != 0 {
            return Err("qualification paths must not be reparse points".into());
        }
        Ok(())
    }
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    if args.len() != 3 {
        return Err("expected <isolated-temp-root> <original-plan> <committed 0|1>".into());
    }
    let committed = match args[2].to_str() {
        Some("0") => false,
        Some("1") => true,
        _ => return Err("committed must be 0 or 1".into()),
    };
    let supplied_root = Path::new(&args[0]);
    regular(supplied_root)?;
    let root = supplied_root.canonicalize()?;
    let temp = std::env::temp_dir().canonicalize()?;
    if root.parent() != Some(temp.as_path())
        || !root
            .file_name()
            .and_then(|v| v.to_str())
            .is_some_and(|v| v.starts_with("morrow-external-guest-content-"))
        || !root.is_dir()
    {
        return Err("only an isolated guest content test directory is accepted".into());
    }
    let supplied_plan = Path::new(&args[1]);
    regular(supplied_plan)?;
    let plan_path = supplied_plan.canonicalize()?;
    if plan_path.parent() != Some(root.as_path()) || !plan_path.is_file() {
        return Err("original plan must be directly inside the test directory".into());
    }
    if fs::metadata(&plan_path)?.len() > morrow_core::file_mutation::MAX_CONTAINER_BYTES as u64 {
        return Err("oversized original plan".into());
    }
    let original = fs::read(plan_path)?;
    let plan = RequestRecord::decode(&original)?;
    let request = plan.request();
    assert_eq!(request.disposition, Disposition::Create);
    assert_eq!(request.content_length, 16 * 1024 * 1024);
    let expected_hash = request.content_sha256.ok_or("missing content digest")?;
    let command = plan.command()?;

    // The managed registry and the same database lease both reject a live owner.
    // Use only an existing protected key and a read-only Store: no initialization
    // or repair can make a failed transaction appear committed during verification.
    regular(&root.join("active-library.lock"))?;
    regular(&root.join("active-library.pb.lz4"))?;
    let registry = Registry::open(&root)?;
    let selected = registry.selected_database()?;
    regular(&selected)?;
    let database = selected.canonicalize()?;
    if database.parent() != Some(root.as_path()) {
        return Err("selected database escaped the isolated directory".into());
    }
    let key_file = key_path(&database)?;
    let lock_file = database.with_file_name("workbench.db.audit-lock");
    regular(&key_file)?;
    regular(&lock_file)?;
    let lease = fs::File::options().read(true).write(true).open(lock_file)?;
    lease.try_lock()?;
    let key = Key::load(&key_file)?;
    let store = Store::open_read_only_audited(&database, key.trust())?;
    store.integrity_check()?;
    let record = store
        .lookup_io_intent(&request.subject, &request.operation_id)?
        .ok_or("missing original intent")?;
    record.matches_command(&command)?;
    assert_eq!(record.phase(), Phase::Prepared);
    assert_eq!(record.recovery(), Recovery::AwaitFreshAuthorization);
    let material = store
        .io_material(&request.subject, &request.operation_id, Kind::Request)?
        .ok_or("missing original request")?;
    assert_eq!(material.payload(), original);
    assert!(matches!(
        store.io_material(&request.subject, &request.operation_id, Kind::Response),
        Err(morrow_core::Error::EvidenceUnavailable)
    ));
    let content = store.file_mutation_content_local_authorized(
        &request.subject,
        &request.operation_id,
        || Ok(()),
    )?;
    let receipt = store.file_mutation_content_receipt_local_authorized(
        &request.subject,
        &request.operation_id,
        || Ok(()),
    )?;
    assert_eq!(content.is_some(), committed, "content commit state");
    assert_eq!(receipt.is_some(), committed, "receipt commit state");
    assert_eq!(store.file_mutation_content_usage()?.0, u64::from(committed));
    if let (Some(content), Some(receipt)) = (content, receipt) {
        assert_eq!(content.request_sha256(), command.request_sha256);
        assert_eq!(content.content().len() as u64, request.content_length);
        assert_eq!(content.content_sha256(), expected_hash);
        let actual: [u8; 32] = Sha256::digest(content.content()).into();
        assert_eq!(actual, expected_hash);
        assert_eq!(receipt.operation_id(), request.operation_id);
        assert_eq!(receipt.subject(), request.subject);
        assert_eq!(receipt.request_sha256(), command.request_sha256);
        assert_eq!(receipt.content_length(), request.content_length);
        assert_eq!(receipt.content_sha256(), expected_hash);
        assert_eq!(receipt.source(), Source::LiveStaging);
        let container_hash: [u8; 32] = Sha256::digest(content.container()).into();
        assert_eq!(receipt.content_container_sha256(), container_hash);
    }
    println!(
        "GUEST_CONTENT_BODY_SHA256={}",
        expected_hash
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>()
    );
    println!(
        "GUEST_CONTENT_STORE_BYTES={}",
        if committed { request.content_length } else { 0 }
    );
    println!("GUEST_CONTENT_STORE_PASS={}", u8::from(committed));
    Ok(())
}

fn main() {
    #[cfg(target_os = "windows")]
    if let Err(error) = run() {
        eprintln!("guest content store verification failed: {error}");
        std::process::exit(1);
    }
    #[cfg(not(target_os = "windows"))]
    {
        eprintln!("Windows protected-store qualification only");
        std::process::exit(1);
    }
}
