use morrow_core::{
    Error,
    plugin_package::{
        MAX_MANIFEST_BYTES, MAX_MODULE_BYTES, MAX_PACKAGE_BYTES, Package,
        proto::{self, Capability},
    },
};
use prost::Message;
use sha2::{Digest, Sha256};
const MODULE: &[u8] = b"\0asm\x01\0\0\0";
fn manifest() -> proto::Manifest {
    Package::manifest_for(
        "org.morrow.example",
        "0.1.9-test.10",
        MODULE,
        vec![Capability::RenameCard],
    )
}
fn package() -> Package {
    Package::build(manifest(), MODULE).unwrap()
}
fn reject(f: impl FnOnce(&mut proto::Manifest)) {
    let mut m = manifest();
    f(&mut m);
    assert!(Package::build(m, MODULE).is_err());
}
fn wrap(raw: &[u8]) -> Vec<u8> {
    let packed = lz4_flex::block::compress(raw);
    let mut out = b"MORROWP1".to_vec();
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&(raw.len() as u32).to_le_bytes());
    out.extend_from_slice(&(packed.len() as u32).to_le_bytes());
    out.extend_from_slice(&Sha256::digest(raw));
    out.extend_from_slice(&packed);
    out
}
#[test]
fn unknown_optional_fields_and_archive_are_preserved() {
    let mut m = manifest().encode_to_vec();
    m.extend_from_slice(&[0xa0, 0x06, 0x07]); // field 100, varint
    let mut raw = proto::Package {
        schema_version: 1,
        manifest: m.clone(),
        module: MODULE.into(),
    }
    .encode_to_vec();
    raw.extend_from_slice(&[0xa0, 0x06, 0x09]);
    let archive = wrap(&raw);
    let p = Package::decode(&archive).unwrap();
    assert_eq!(p.archive(), archive);
    assert_eq!(p.manifest_bytes(), m);
    assert_eq!(p.module(), MODULE);
    assert_eq!(p.digest(), <[u8; 32]>::from(Sha256::digest(&archive)));
    assert_eq!(Package::from_parts(&m, MODULE).unwrap().manifest_bytes(), m);
}
#[test]
fn unsupported_versions_contracts_and_required_features_fail_closed() {
    reject(|m| m.schema_version = 2);
    reject(|m| m.guest_abi_version = 2);
    reject(|m| m.runtime_protocol_version += 1);
    reject(|m| m.runtime_schema_sha256[0] ^= 1);
    reject(|m| m.content_schema_sha256.clear());
    reject(|m| m.entrypoint = "other".into());
    reject(|m| m.required_features.push("dependencies-v1".into()));
    let raw = proto::Package {
        schema_version: 2,
        manifest: manifest().encode_to_vec(),
        module: MODULE.into(),
    }
    .encode_to_vec();
    assert!(matches!(
        Package::decode(&wrap(&raw)),
        Err(Error::UnsupportedVersion)
    ));
}
#[test]
fn invalid_metadata_capabilities_and_budgets_rejected() {
    for id in ["", "../escape", "C:drive", "bad\nid"] {
        reject(|m| m.package_id = id.into());
    }
    for v in ["", "latest", "1.2", "01.2.3"] {
        reject(|m| m.package_version = v.into());
    }
    reject(|m| m.display_name.clear());
    reject(|m| m.display_name = "a\nb".into());
    reject(|m| m.requested_capabilities = vec![0]);
    reject(|m| m.requested_capabilities = vec![999]);
    reject(|m| m.requested_capabilities = vec![1, 1]);
    reject(|m| m.budget = None);
    for f in [0, 100_000_001] {
        reject(|m| m.budget.as_mut().unwrap().fuel = f);
    }
    for b in [0, 65535, 65537, 64 * 1024 * 1024 + 65536] {
        reject(|m| m.budget.as_mut().unwrap().memory_bytes = b);
    }
    reject(|m| m.budget.as_mut().unwrap().host_calls = 1025);
    let mut m = manifest();
    m.requested_capabilities.clear();
    m.budget.as_mut().unwrap().host_calls = 0;
    assert!(Package::build(m, MODULE).is_ok());
}
#[test]
fn module_digest_header_and_size_checked() {
    reject(|m| m.module_sha256[0] ^= 1);
    reject(|m| m.module_sha256.clear());
    let wrong = b"not wasm";
    assert!(Package::build(Package::manifest_for("x", "1.0.0", wrong, vec![]), wrong).is_err());
    assert!(matches!(
        Package::from_parts(&vec![0; MAX_MANIFEST_BYTES + 1], MODULE),
        Err(Error::Limit)
    ));
    assert!(matches!(
        Package::from_parts(&[], &vec![0; MAX_MODULE_BYTES + 1]),
        Err(Error::Limit)
    ));
}
#[test]
fn corrupt_truncated_trailing_and_inflated_containers_rejected() {
    let p = package();
    let a = p.archive();
    for n in [0, 7, 49, a.len() - 1] {
        assert!(Package::decode(&a[..n]).is_err());
    }
    let mut corrupt = a.to_vec();
    corrupt[18] ^= 1;
    assert!(matches!(Package::decode(&corrupt), Err(Error::Integrity)));
    let mut trailing = a.to_vec();
    trailing.push(0);
    assert!(Package::decode(&trailing).is_err());
    let mut bomb = a.to_vec();
    bomb[10..14].copy_from_slice(&u32::MAX.to_le_bytes());
    assert!(matches!(Package::decode(&bomb), Err(Error::Limit)));
    assert!(matches!(
        Package::decode(&vec![0; MAX_PACKAGE_BYTES + 1]),
        Err(Error::Limit)
    ));
    assert!(Package::from_parts(&[0x0b, 0x0c], MODULE).is_err()); // groups forbidden
    let mut flood = manifest().encode_to_vec();
    for _ in 0..257 {
        flood.extend_from_slice(&[0xa0, 0x06, 0]);
    }
    assert!(Package::from_parts(&flood, MODULE).is_err());
}
#[cfg(not(target_arch = "wasm32"))]
mod native {
    use super::*;
    use morrow_core::{
        content::CardRecord,
        dispatch::HostRuntime,
        lifecycle::GrantKind,
        plugin_package::catalog::Catalog,
        response::{Failure, Outcome, Response},
        runtime::Command,
        store::{EventBudget, Store},
    };
    #[test]
    fn install_is_idempotent_and_detects_existing_tamper_without_overwrite() {
        let dir = tempfile::tempdir().unwrap();
        let cat = Catalog::open(dir.path()).unwrap();
        let p = package();
        let path = cat.install(&p).unwrap();
        assert_eq!(cat.install(&p).unwrap(), path);
        assert_eq!(cat.load(p.digest()).unwrap().archive(), p.archive());
        assert_eq!(path.file_stem().unwrap().to_string_lossy().len(), 64);
        let mut m = manifest();
        m.package_version = "0.1.9-test.11".into();
        let newer = Package::build(m, MODULE).unwrap();
        let second = cat.install(&newer).unwrap();
        assert_ne!(second, path);
        std::fs::write(&path, newer.archive()).unwrap();
        assert!(cat.load(p.digest()).is_err());
        assert!(cat.install(&p).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), newer.archive());
    }
    #[test]
    fn concurrent_same_package_install_publishes_one_complete_file() {
        let dir = tempfile::tempdir().unwrap();
        std::thread::scope(|scope| {
            let mut workers = vec![];
            for _ in 0..4 {
                let root = dir.path();
                workers.push(
                    scope.spawn(move || Catalog::open(root).unwrap().install(&package()).unwrap()),
                );
            }
            let paths: Vec<_> = workers.into_iter().map(|t| t.join().unwrap()).collect();
            assert!(paths.iter().all(|p| p == &paths[0]));
        });
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1);
        Catalog::open(dir.path())
            .unwrap()
            .load(package().digest())
            .unwrap();
    }
    #[test]
    fn declarations_never_grant_and_cannot_be_exceeded() {
        let dir = tempfile::tempdir().unwrap();
        let mut store = Store::open(&dir.path().join("db"), EventBudget::default()).unwrap();
        store
            .create_local(
                "seed",
                &CardRecord::new("card", "note", 1, "old", vec![]).unwrap(),
            )
            .unwrap();
        let mut host = HostRuntime::new(store).unwrap();
        let mut m = manifest();
        m.requested_capabilities = vec![Capability::ReadSummary as i32];
        let p = Package::build(m, MODULE).unwrap();
        let mut a = host.connect_package(&p).unwrap();
        let b = host.connect_package(&p).unwrap();
        assert_eq!(a.package_digest(), Some(p.digest()));
        assert!(
            host.grant(&mut a, GrantKind::Rename, "card", 100, 0)
                .is_err()
        );
        assert!(
            host.grant(&mut a, GrantKind::QueryOperation, "card", 100, 0)
                .is_err()
        );
        assert!(
            host.grant_attachment(&mut a, "card", "attachment", 100, 0)
                .is_err()
        );
        let request = Command::ReadSummary {
            request_id: "read".into(),
            card_id: "card".into(),
        }
        .encode()
        .unwrap();
        let denied = Outcome::Rejected(Failure::Denied);
        assert_eq!(
            Response::decode(&host.dispatch(&a, &request, || 1).unwrap())
                .unwrap()
                .outcome,
            denied
        );
        host.grant(&mut a, GrantKind::ReadSummary, "card", 100, 1)
            .unwrap();
        assert!(matches!(
            Response::decode(&host.dispatch(&a, &request, || 2).unwrap())
                .unwrap()
                .outcome,
            Outcome::Summary(_)
        ));
        assert_eq!(
            Response::decode(&host.dispatch(&b, &request, || 2).unwrap())
                .unwrap()
                .outcome,
            denied
        );
        host.disconnect(&a).unwrap();
        assert_eq!(
            Response::decode(&host.dispatch(&a, &request, || 3).unwrap())
                .unwrap()
                .outcome,
            denied
        );
    }
}

#[test]
fn task_abi_requires_exact_task_contract_and_cannot_be_mislabelled_legacy() {
    let m = Package::manifest_for_task("tasks", "1.0.0", MODULE, vec![Capability::RenameCard]);
    assert!(Package::build(m.clone(), MODULE).is_ok());
    let mut bad = m.clone();
    bad.task_schema_sha256[0] ^= 1;
    assert!(Package::build(bad, MODULE).is_err());
    let mut legacy = m.clone();
    legacy.guest_abi_version = 1;
    assert!(Package::build(legacy, MODULE).is_err());
    let mut future = m;
    future.guest_abi_version = 3;
    assert!(Package::build(future, MODULE).is_err());
}

fn handler() -> proto::TransformHandler {
    proto::TransformHandler {
        handler: "bytes.reverse".into(),
        input_type: "bytes".into(),
        output_type: "bytes".into(),
        max_input_bytes: 65536,
        max_output_bytes: 65536,
    }
}
#[test]
fn transform_declarations_require_supported_feature_and_unique_bounded_handlers() {
    let good = Package::manifest_for_transform("transforms", "1.0.0", MODULE, vec![handler()]);
    let package = Package::build(good.clone(), MODULE).unwrap();
    assert_eq!(
        Package::decode(package.archive())
            .unwrap()
            .manifest()
            .transform_handlers,
        good.transform_handlers
    );
    for case in 0..11 {
        let mut m = good.clone();
        match case {
            0 => m.required_features.clear(),
            1 => m.transform_handlers.clear(),
            2 => {
                m.guest_abi_version = 1;
                m.task_schema_sha256.clear();
            }
            3 => m.required_features.push("transform-handlers-v1".into()),
            4 => m.transform_handlers.push(handler()),
            5 => m.transform_handlers[0].handler = "../bad".into(),
            6 => m.transform_handlers[0].input_type.clear(),
            7 => m.transform_handlers[0].output_type = "bad:type".into(),
            8 => m.transform_handlers[0].max_input_bytes = 65537,
            9 => m.transform_handlers[0].max_output_bytes = 65537,
            _ => {
                m.transform_handlers = (0..17)
                    .map(|n| {
                        let mut h = handler();
                        h.handler = format!("h{n}");
                        h
                    })
                    .collect();
            }
        }
        assert!(Package::build(m, MODULE).is_err(), "case {case}");
    }
    let mut max = good;
    max.transform_handlers = (0..16)
        .map(|n| {
            let mut h = handler();
            h.handler = format!("h{n}");
            h
        })
        .collect();
    assert!(Package::build(max, MODULE).is_ok());
}
#[test]
fn transform_resolution_is_package_scoped_and_enforces_declared_input() {
    use morrow_core::task::Transform;
    let mut h = handler();
    h.max_input_bytes = 0;
    h.max_output_bytes = 0;
    let p = Package::build(
        Package::manifest_for_transform("empty", "1.0.0", MODULE, vec![h]),
        MODULE,
    )
    .unwrap();
    let mut input = Transform {
        handler: "bytes.reverse".into(),
        input_type: "bytes".into(),
        output_type: "bytes".into(),
        input: vec![],
    };
    assert_eq!(p.transform_handler(&input).unwrap().max_output_bytes, 0);
    input.input.push(1);
    assert!(p.transform_handler(&input).is_err());
    input.input.clear();
    input.output_type = "other".into();
    assert!(p.transform_handler(&input).is_err());
    input.output_type = "bytes".into();
    input.input_type = "other".into();
    assert!(p.transform_handler(&input).is_err());
    input.input_type = "bytes".into();
    input.handler = "unknown".into();
    assert!(p.transform_handler(&input).is_err());
    input.handler = "bytes.reverse".into();
    let legacy = Package::build(
        Package::manifest_for_task("legacy", "1.0.0", MODULE, vec![]),
        MODULE,
    )
    .unwrap();
    assert!(legacy.transform_handler(&input).is_err());
    let mut changed = p.manifest().clone();
    changed.transform_handlers[0].max_input_bytes = 1;
    assert_ne!(
        Package::build(changed, MODULE).unwrap().digest(),
        p.digest()
    );
}
