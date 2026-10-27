//! Read a known closed protected qualification library; never replay an owner.
#[cfg(windows)]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    use morrow_audit::{
        library::Registry,
        session::{OpenMode, Session},
    };
    use sha2::{Digest, Sha256};
    use std::{fs::File, io::Read, path::Path};
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    if args.len() != 3 {
        return Err("usage: check_blob_text_fixture ROOT EXPECTED_UTF8_FILE EXPECTED_COUNT".into());
    }
    let root = Path::new(&args[0]);
    let input = Path::new(&args[1]);
    if !root.is_absolute()
        || !root.is_dir()
        || !root.starts_with("D:/build_cache")
        || !input.is_absolute()
        || !input.starts_with("D:/build_cache")
    {
        return Err("explicit qualification paths required".into());
    }
    let count: usize = args[2].to_str().ok_or("count")?.parse()?;
    if count > 1 {
        return Err("one original text upload only".into());
    }
    let mut expected = Vec::new();
    File::open(input)?.take(32769).read_to_end(&mut expected)?;
    if expected.len() > 32768 || std::str::from_utf8(&expected).is_err() {
        return Err("bounded UTF8 expected".into());
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
        let digest: [u8; 32] = Sha256::digest(&expected).into();
        if exported != *info
            || info.byte_length != expected.len() as u64
            || info.sha256 != digest
            || info.retired_at_unix_ms.is_some()
            || bytes != expected
        {
            return Err("original protected text mismatch".into());
        }
    }
    if store.pending_usage()? != (0, 0) {
        return Err("pending original owner evidence".into());
    }
    println!(
        "VERIFIED_ORIGINAL_TEXT_UPLOAD length={} objects={count} exact_bytes_sha=true pending=0",
        expected.len()
    );
    Ok(())
}
#[cfg(not(windows))]
fn main() {
    eprintln!("Windows protected fixture verification only");
    std::process::exit(2);
}
