//! Creates one fresh, protected local history fixture for process-transport checks.
//! It never opens a previous test owner or retries the original upload.
#[cfg(windows)]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    use morrow_audit::session::{OpenMode, Session};
    use morrow_core::{
        blob_stage::Command,
        blob_upload_journal as journal,
        plugin_package::{Package, proto::TransformHandler},
    };
    use morrow_workbench_host::Workbench;
    use sha2::{Digest, Sha256};
    use std::{
        fs,
        io::{Read, Write},
        path::Path,
    };
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    if args.len() != 3 {
        return Err(
            "usage: prepare_blob_history_fixture NEW_ROOT GUEST_WASM EXPECTED_SHA256".into(),
        );
    }
    let root = Path::new(&args[0]);
    let wasm = Path::new(&args[1]);
    for path in [root, wasm] {
        if !path.is_absolute() || !path.starts_with("D:/build_cache") {
            return Err("explicit D build-cache fixture paths required".into());
        }
        for ancestor in path.ancestors().filter(|p| p.exists()) {
            use std::os::windows::fs::MetadataExt;
            if fs::symlink_metadata(ancestor)?.file_attributes() & 1024 != 0 {
                return Err("fixture path contains a reparse point".into());
            }
        }
    }
    let expected = args[2].to_str().ok_or("expected guest digest")?;
    if expected.len() != 64 || !expected.bytes().all(|c| c.is_ascii_hexdigit()) {
        return Err("expected guest SHA256 required".into());
    }
    let mut module = Vec::new();
    fs::File::open(wasm)?
        .take(4 * 1024 * 1024 + 1)
        .read_to_end(&mut module)?;
    if module.len() > 4 * 1024 * 1024 || format!("{:x}", Sha256::digest(&module)) != expected {
        return Err("guest bytes differ from the reviewed artifact".into());
    }
    let id = "test.history.recipient";
    let package = Package::build(
        Package::manifest_for_transform(
            id,
            "1.0.0",
            &module,
            ["history.inspect", "history.fixture.inspect"]
                .into_iter()
                .map(|handler| TransformHandler {
                    handler: handler.into(),
                    input_type: morrow_blob_upload_history_v1::INPUT_TYPE.into(),
                    output_type: morrow_blob_upload_history_v1::INPUT_TYPE.into(),
                    max_input_bytes: 8192,
                    max_output_bytes: 8192,
                })
                .collect(),
        ),
        &module,
    )?;
    // create_dir is exclusive: an existing or retained fixture is never reused.
    fs::create_dir(root)?;
    let db = root.join("library.db");
    let mut session = Session::open(&db, Default::default(), OpenMode::Initialize)?;
    let request = journal::Request {
        submission: [21; 32],
        package_id: "old.uploader".into(),
        package_digest: [22; 32],
        registry_revision: 7,
        nomination_ref: [23; 32],
        command: Command {
            owner_domain: [24; 32],
            operation_id: "original-unknown".into(),
            byte_length: 0,
            sha256: Sha256::digest([]).into(),
        },
        handler: "old.upload".into(),
        input_sha256: [25; 32],
        binding_sha256: [26; 32],
    };
    session
        .runtime()
        .store_local_mut()
        .reserve_blob_upload_local_authorized(&request, 10, || Ok(()))?;
    let before = session
        .runtime()
        .store_local_mut()
        .observe_blob_upload_local_authorized(
            &request,
            journal::OriginalResult {
                outcome: journal::Outcome::Error(7),
                unknown: true,
                calls: 0,
                wire_bytes: 0,
                native_work_bytes: 0,
                execution: None,
            },
            11,
            || Ok(()),
        )?;
    if session.flush(16)?.more_pending {
        return Err("fixture has pending journal work".into());
    }
    drop(session);
    let mut host = Workbench::open(&db, None)?;
    let rev = host.catalog_page("", None)?.revision;
    host.import_plugin_bytes(package.archive(), &package.digest(), rev)?;
    let rev = host.catalog_page("", None)?.revision;
    host.configure_external(id, &package.digest(), rev, &[], true)?;
    let revision = host.catalog_page("", None)?.revision;
    if host.inspect_persisted_blob_upload(&[21; 32])?.record != before || !before.original_unknown()
    {
        return Err("original history was changed by fixture preparation".into());
    }
    host.finish()?;
    let digest = package
        .digest()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<String>();
    let receipt = serde_json::json!({"status":"FRESH_PROTECTED_HISTORY_FIXTURE_CLOSED",
        "database":db,"package_id":id,"package_digest":digest,"registry_revision":revision.to_string(),
        "submission":"15".repeat(32),"guest_sha256":expected,"original_unknown":true,
        "prior_owner_contact":false,"original_upload_retried":false,"history_delivery_started":false});
    let mut file = fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(root.join("fixture.json"))?;
    file.write_all(serde_json::to_string_pretty(&receipt)?.as_bytes())?;
    file.sync_all()?;
    println!("FRESH_PROTECTED_HISTORY_FIXTURE_CLOSED");
    Ok(())
}
#[cfg(not(windows))]
fn main() {
    eprintln!("Windows protected fixture only");
    std::process::exit(2);
}
