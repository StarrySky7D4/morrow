//! Inspect only an explicitly named closed qualification library. Generated
//! operation identities are not reconstructed, renewed, or submitted here.
#[cfg(windows)]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    use morrow_audit::{
        library::Registry,
        session::{OpenMode, Session},
    };
    use sha2::{Digest, Sha256};
    use std::path::Path;
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    if args.len() != 3 {
        return Err("usage: check_blob_plan_fixture ROOT LENGTH EXPECTED_COUNT".into());
    }
    let root = Path::new(&args[0]);
    if !root.is_absolute() || !root.is_dir() || !root.starts_with("D:/build_cache") {
        return Err("explicit qualification root required".into());
    }
    let length: usize = args[1].to_str().ok_or("length")?.parse()?;
    let count: usize = args[2].to_str().ok_or("count")?.parse()?;
    if ![150017, 16777216].contains(&length) || count > 1 {
        return Err("synthetic fixture dimensions".into());
    }
    let database = Registry::open(root)?.selected_database()?;
    let session = Session::open(&database, Default::default(), OpenMode::Existing)?;
    let store = session.runtime_ref().store_local();
    store.integrity_check()?;
    let records = store.list_blobs_local("", 2)?;
    if records.len() != count {
        return Err("unexpected stored object count".into());
    }
    if let Some(info) = records.first() {
        let mut bytes = Vec::new();
        let exported = store.export_blob_local(&info.id, &mut bytes)?;
        let expected: [u8; 32] = Sha256::digest(vec![73; length]).into();
        if exported != *info
            || info.byte_length != length as u64
            || info.sha256 != expected
            || info.retired_at_unix_ms.is_some()
            || bytes != vec![73; length]
        {
            return Err("original protected content mismatch".into());
        }
    }
    if store.pending_usage()? != (0, 0) {
        return Err("pending original owner evidence".into());
    }
    println!(
        "VERIFIED_ORIGINAL_PLANNED_UPLOAD length={length} objects={count} exact_bytes_sha=true pending=0"
    );
    Ok(())
}
#[cfg(not(windows))]
fn main() {
    eprintln!("Windows protected fixture verification only");
    std::process::exit(2);
}
