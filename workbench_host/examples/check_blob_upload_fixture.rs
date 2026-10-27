//! Read an explicitly named, closed synthetic qualification library. No upload
//! authority, new worker or retry is constructed by this observer.
#[cfg(windows)]
use morrow_audit::{
    library::Registry,
    session::{OpenMode, Session},
};
#[cfg(windows)]
use morrow_blob_transfer_v1::{Identity, Limits};
#[cfg(windows)]
use morrow_plugin_runtime::blob_upload::Spec;
#[cfg(windows)]
use sha2::{Digest, Sha256};
#[cfg(windows)]
use std::path::Path;
#[cfg(windows)]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    if args.len() != 3 {
        return Err("usage: check_blob_upload_fixture ROOT LENGTH staged|released".into());
    }
    let length: usize = args[1].to_str().ok_or("length")?.parse()?;
    if ![150017, 16 * 1024 * 1024].contains(&length) {
        return Err("synthetic size".into());
    }
    let released = match args[2].to_str() {
        Some("staged") => false,
        Some("released") => true,
        _ => return Err("phase".into()),
    };
    let database = Registry::open(Path::new(&args[0]))?.selected_database()?;
    let session = Session::open(&database, Default::default(), OpenMode::Existing)?;
    let store = session.runtime_ref().store_local();
    store.integrity_check()?;
    let spec = Spec {
        owner_domain: [72; 32],
        identity: Identity {
            transfer_epoch: [71; 32],
            object_ref: [2; 32],
            operation_id: [3; 32],
        },
        byte_length: length as u64,
        sha256: Sha256::digest(vec![42; length]).into(),
        limits: Limits::default(),
    };
    let record = store
        .inspect_blob_stage_local(&spec.command())?
        .ok_or("original history missing")?;
    if record.is_released() != released {
        return Err("original history mismatch".into());
    }
    if !released {
        let mut bytes = Vec::new();
        store.export_blob_local(&record.blob().id, &mut bytes)?;
        if bytes.len() != length
            || <[u8; 32]>::from(Sha256::digest(&bytes)) != spec.sha256
            || bytes.iter().any(|b| *b != 42)
        {
            return Err("original protected bytes mismatch".into());
        }
    }
    if store.pending_usage()? != (0, 0) {
        return Err("pending original audit".into());
    }
    println!("VERIFIED_ORIGINAL_PROTECTED_UPLOAD length={length} released={released} pending=0");
    Ok(())
}

#[cfg(not(windows))]
fn main() {
    eprintln!("Windows protected fixture verification only");
    std::process::exit(2);
}
