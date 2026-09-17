//! Synthetic unsigned records test the codec, not the truth of a compiler run.
use morrow_audit::build_receipt::{self as receipt, Error, proto};
use prost::Message;
use sha2::{Digest, Sha256};

fn file(path: &str, bytes: &[u8]) -> proto::FileEntry {
    proto::FileEntry {
        path: path.into(),
        size: bytes.len() as u64,
        sha256: Sha256::digest(bytes).to_vec(),
    }
}
fn good() -> proto::BuildReceipt {
    let sources = vec![
        file("Cargo.toml", b"synthetic"),
        file("src/main.rs", b"fn main() {}"),
    ];
    proto::BuildReceipt {
        version: 1,
        source_head: "a".repeat(40),
        source_dirty: true,
        source_root_sha256: receipt::source_digest(&sources).unwrap().to_vec(),
        sources,
        profile: "network-node-release".into(),
        target: "x86_64-pc-windows-msvc".into(),
        features: vec!["plugin-adapter".into()],
        rustc_version: "fixture rustc\nhost: fixture".into(),
        cargo_version: "fixture cargo".into(),
        collector_sha256: vec![7; 32],
        commands: vec![proto::CommandRecord {
            label: "build".into(),
            program: "C:/Program Files/Rust/cargo.exe".into(),
            args: vec![
                "build".into(),
                "--locked".into(),
                "--offline".into(),
                "".into(),
            ],
            exit_code: 0,
            completed: true,
            stdout: Some(file("logs/build.stdout", b"")),
            stderr: Some(file("logs/build.stderr", b"fixture only")),
        }],
        artifacts: vec![file("artifacts/program.exe", b"not an executable")],
        build_succeeded: true,
        started_unix_ms: 1000,
        finished_unix_ms: 1001,
    }
}
fn pack(raw: &[u8]) -> Vec<u8> {
    let packed = lz4_flex::block::compress(raw);
    let mut bytes = b"MORROWB1".to_vec();
    bytes.extend_from_slice(&1u16.to_le_bytes());
    bytes.extend_from_slice(&(raw.len() as u32).to_le_bytes());
    bytes.extend_from_slice(&(packed.len() as u32).to_le_bytes());
    bytes.extend_from_slice(&Sha256::digest(raw));
    bytes.extend_from_slice(&packed);
    bytes
}
fn varint(mut value: u64) -> Vec<u8> {
    let mut bytes = Vec::new();
    while value >= 128 {
        bytes.push(value as u8 | 0x80);
        value >>= 7;
    }
    bytes.push(value as u8);
    bytes
}
fn bytes_field(tag: u64, value: &[u8]) -> Vec<u8> {
    let mut bytes = varint(tag * 8 + 2);
    bytes.extend(varint(value.len() as u64));
    bytes.extend_from_slice(value);
    bytes
}
fn malformed(value: &proto::BuildReceipt) -> Result<proto::BuildReceipt, Error> {
    receipt::decode(&pack(&value.encode_to_vec()))
}
fn reject(value: &proto::BuildReceipt) {
    assert!(receipt::encode(value).is_err());
    assert!(malformed(value).is_err());
}

#[test]
fn success_and_failed_build_roundtrip_preserve_all_facts() {
    let success = good();
    assert_eq!(
        receipt::decode(&receipt::encode(&success).unwrap()).unwrap(),
        success
    );
    let mut failed = success;
    failed.build_succeeded = false;
    failed.commands[0].exit_code = 101;
    failed.artifacts.clear();
    assert_eq!(
        receipt::decode(&receipt::encode(&failed).unwrap()).unwrap(),
        failed
    );
    failed.commands[0].exit_code = -9;
    assert_eq!(
        receipt::decode(&receipt::encode(&failed).unwrap()).unwrap(),
        failed
    );
}

#[test]
fn success_cannot_be_claimed_for_nonbuild_incomplete_failed_or_artifactless_commands() {
    let mut value = good();
    value.commands[0].exit_code = 1;
    reject(&value);
    value = good();
    value.commands[0].completed = false;
    reject(&value);
    value = good();
    value.commands[0].label = "rustc-version".into();
    reject(&value);
    value = good();
    value.artifacts.clear();
    reject(&value);
    value = good();
    value.commands.clear();
    reject(&value);
    value = good();
    value.commands[0].stdout = None;
    reject(&value);
    value = good();
    value.commands[0].stderr = None;
    reject(&value);
}

#[test]
fn source_digest_has_independent_domain_separated_vector_and_detects_each_fact_change() {
    let sources = vec![file("a", b"x"), file("目录/b", b"yz")];
    // Independent explicit field layout; no Protobuf bytes enter this vector.
    let mut bytes = b"Morrow/build-sources/v1\0".to_vec();
    bytes.extend_from_slice(&2u64.to_le_bytes());
    for (path, size, content) in [
        ("a", 1u64, b"x".as_slice()),
        ("目录/b", 2, b"yz".as_slice()),
    ] {
        bytes.extend_from_slice(&(path.len() as u32).to_le_bytes());
        bytes.extend_from_slice(path.as_bytes());
        bytes.extend_from_slice(&size.to_le_bytes());
        bytes.extend_from_slice(&Sha256::digest(content));
    }
    let expected: [u8; 32] = Sha256::digest(bytes).into();
    assert_eq!(receipt::source_digest(&sources).unwrap(), expected);
    for field in 0..3 {
        let mut changed = sources.clone();
        match field {
            0 => changed[0].path = "b".into(),
            1 => changed[0].size += 1,
            _ => changed[0].sha256[0] ^= 1,
        }
        assert_ne!(receipt::source_digest(&changed).unwrap(), expected);
    }
    let mut value = good();
    value.sources[0].size += 1;
    assert_eq!(receipt::encode(&value), Err(Error::Integrity));
    assert_eq!(malformed(&value), Err(Error::Integrity));
}

#[test]
fn sources_must_be_nonempty_unique_and_sorted_without_silent_repair() {
    let mut value = good();
    value.sources.clear();
    reject(&value);
    value = good();
    value.sources.reverse();
    reject(&value);
    value = good();
    value.sources.push(value.sources[0].clone());
    reject(&value);
    assert!(receipt::source_digest(&[]).is_err());
}

#[test]
fn relative_paths_reject_escape_absolute_and_ambiguous_forms_everywhere() {
    for bad in [
        "",
        "/",
        "/tmp/a",
        "C:/a",
        "C:a",
        "\\server\\file",
        "a\\b",
        "../a",
        "a/../b",
        "./a",
        "a//b",
        "a/",
        "a/.",
        "a/..",
        "a /b",
        "a./b",
        "a\0b",
        "a\nb",
    ] {
        for location in 0..3 {
            let mut value = good();
            match location {
                0 => value.sources[0].path = bad.into(),
                1 => value.commands[0].stdout.as_mut().unwrap().path = bad.into(),
                _ => value.artifacts[0].path = bad.into(),
            }
            reject(&value);
        }
    }
    let mut value = good();
    value.artifacts[0].path = "artifacts/中文 路径/result.exe".into();
    assert!(receipt::encode(&value).is_ok());
}

#[test]
fn duplicate_features_labels_logs_and_artifacts_are_rejected() {
    let mut value = good();
    value.features.push(value.features[0].clone());
    reject(&value);
    value = good();
    value.commands.push(value.commands[0].clone());
    reject(&value);
    value = good();
    value.commands[0].stderr = value.commands[0].stdout.clone();
    reject(&value);
    value = good();
    value.artifacts.push(value.artifacts[0].clone());
    reject(&value);
    value = good();
    value.artifacts[0].path = value.commands[0].stdout.as_ref().unwrap().path.clone();
    reject(&value);
}

#[test]
fn invalid_hashes_head_versions_times_and_missing_metadata_are_rejected() {
    for tag in 0..12 {
        let mut value = good();
        match tag {
            0 => value.source_head = "A".repeat(40),
            1 => value.source_head = "g".repeat(40),
            2 => value.collector_sha256.pop().map(|_| ()).unwrap(),
            3 => value.source_root_sha256.clear(),
            4 => value.commands[0].stderr.as_mut().unwrap().sha256.clear(),
            5 => value.artifacts[0].sha256.push(0),
            6 => value.finished_unix_ms = value.started_unix_ms - 1,
            7 => value.started_unix_ms = 0,
            8 => value.profile.clear(),
            9 => value.target.clear(),
            10 => value.rustc_version.clear(),
            _ => value.cargo_version.clear(),
        }
        reject(&value);
    }
    let mut value = good();
    value.version = 2;
    assert_eq!(receipt::encode(&value), Err(Error::Version));
    assert_eq!(malformed(&value), Err(Error::Version));
}

#[test]
fn container_hash_truncation_trailing_and_unknown_format_fail() {
    let valid = receipt::encode(&good()).unwrap();
    for length in [0, 7, 8, 49, valid.len() - 1] {
        assert!(receipt::decode(&valid[..length]).is_err());
    }
    let mut changed = valid.clone();
    changed[18] ^= 1;
    assert_eq!(receipt::decode(&changed), Err(Error::Integrity));
    changed = valid.clone();
    changed[0] ^= 1;
    assert_eq!(receipt::decode(&changed), Err(Error::Invalid));
    changed = valid.clone();
    changed[8] = 2;
    assert_eq!(receipt::decode(&changed), Err(Error::Version));
    changed = valid.clone();
    changed.push(0);
    assert_eq!(receipt::decode(&changed), Err(Error::Invalid));
    changed = valid.clone();
    changed.extend_from_slice(&valid);
    assert_eq!(receipt::decode(&changed), Err(Error::Invalid));
}

#[test]
fn raw_length_and_expansion_bombs_are_rejected_before_unbounded_allocation() {
    let valid = receipt::encode(&good()).unwrap();
    for length in [0u32, receipt::MAX_RAW_BYTES as u32 + 1, u32::MAX] {
        let mut changed = valid.clone();
        changed[10..14].copy_from_slice(&length.to_le_bytes());
        assert_eq!(receipt::decode(&changed), Err(Error::Limit));
    }
    let mut changed = valid;
    changed[10..14].copy_from_slice(&1u32.to_le_bytes());
    assert!(receipt::decode(&changed).is_err());
    assert_eq!(
        receipt::decode(&vec![0; receipt::MAX_CONTAINER_BYTES + 1]),
        Err(Error::Limit)
    );
}

#[test]
fn duplicate_unknown_fields_and_wrong_wire_types_reject_even_with_valid_outer_hash() {
    let original = good().encode_to_vec();
    for suffix in [
        vec![8, 1],
        bytes_field(6, b"again"),
        vec![17 << 3, 1],
        vec![11, 12],
        bytes_field(14, b"bad bool wire"),
    ] {
        let mut raw = original.clone();
        raw.extend(suffix);
        assert!(receipt::decode(&pack(&raw)).is_err());
    }
    let mut duplicate_file = file("z", b"x").encode_to_vec();
    duplicate_file.extend(bytes_field(1, b"again"));
    let mut raw = original.clone();
    raw.extend(bytes_field(5, &duplicate_file));
    assert_eq!(receipt::decode(&pack(&raw)), Err(Error::Invalid));
    let mut duplicate_command = good().commands.remove(0).encode_to_vec();
    duplicate_command.extend(bytes_field(6, &file("logs/other", b"").encode_to_vec()));
    let mut raw = original;
    raw.extend(bytes_field(12, &duplicate_command));
    assert_eq!(receipt::decode(&pack(&raw)), Err(Error::Invalid));
}

#[test]
fn invalid_varints_scalars_and_huge_field_lengths_fail_closed() {
    let mut overflowing_version = vec![8];
    overflowing_version.extend(varint(u32::MAX as u64 + 1));
    let mut invalid_boolean = good().encode_to_vec();
    invalid_boolean.extend([14 << 3, 2]);
    let mut impossible_length = vec![18];
    impossible_length.extend(varint(u64::MAX));
    for raw in [
        overflowing_version,
        invalid_boolean,
        impossible_length,
        vec![8, 0x81, 0],
        vec![0],
        vec![0xff; 12],
    ] {
        assert!(receipt::decode(&pack(&raw)).is_err());
    }
    let mut value = good();
    value.commands[0].args.push("\0".into());
    reject(&value);
}

#[test]
fn quantity_limits_include_wire_preflight_and_valid_maximum_sources() {
    let mut value = good();
    value.sources = (0..receipt::MAX_SOURCES)
        .map(|i| file(&format!("source/{i:05}"), b""))
        .collect();
    value.source_root_sha256 = receipt::source_digest(&value.sources).unwrap().to_vec();
    assert_eq!(
        receipt::decode(&receipt::encode(&value).unwrap()).unwrap(),
        value
    );
    value.sources.push(file("source/99999", b""));
    assert_eq!(receipt::encode(&value), Err(Error::Limit));
    assert_eq!(malformed(&value), Err(Error::Limit));
    let mut value = good();
    value.commands[0].args = vec!["x".into(); receipt::MAX_ARGUMENTS + 1];
    assert_eq!(receipt::encode(&value), Err(Error::Limit));
    assert_eq!(malformed(&value), Err(Error::Limit));
    let mut value = good();
    value.features = (0..=receipt::MAX_FEATURES)
        .map(|i| format!("feature{i}"))
        .collect();
    assert_eq!(receipt::encode(&value), Err(Error::Limit));
    assert_eq!(malformed(&value), Err(Error::Limit));
    let mut value = good();
    value.commands = vec![value.commands[0].clone(); receipt::MAX_COMMANDS + 1];
    assert_eq!(receipt::encode(&value), Err(Error::Limit));
    assert_eq!(malformed(&value), Err(Error::Limit));
    let mut value = good();
    value.artifacts = vec![value.artifacts[0].clone(); receipt::MAX_ARTIFACTS + 1];
    assert_eq!(receipt::encode(&value), Err(Error::Limit));
    assert_eq!(malformed(&value), Err(Error::Limit));
}

#[test]
fn individual_text_and_total_raw_limits_are_enforced() {
    let mut value = good();
    value.commands[0].args = vec!["x".repeat(receipt::MAX_ARGUMENT_BYTES + 1)];
    assert_eq!(receipt::encode(&value), Err(Error::Limit));
    assert_eq!(malformed(&value), Err(Error::Limit));
    let mut value = good();
    value.artifacts[0].path = "x".repeat(receipt::MAX_PATH_BYTES + 1);
    assert_eq!(receipt::encode(&value), Err(Error::Limit));
    assert_eq!(malformed(&value), Err(Error::Limit));
    let mut value = good();
    value.sources = (0..receipt::MAX_SOURCES)
        .map(|i| file(&format!("source/{i:05}/{}", "x".repeat(450)), b""))
        .collect();
    value.source_root_sha256 = receipt::source_digest(&value.sources).unwrap().to_vec();
    assert!(value.encoded_len() > receipt::MAX_RAW_BYTES);
    assert_eq!(receipt::encode(&value), Err(Error::Limit));
    assert_eq!(malformed(&value), Err(Error::Limit));
}

#[test]
fn receipt_is_explicitly_unsigned_and_does_not_validate_real_artifact_content() {
    let mut value = good();
    value.artifacts[0] = file("artifacts/invented.exe", b"any synthetic bytes");
    // This is intentionally possible: the codec authenticates no author and opens
    // no files. A verifier must compare every recorded hash with the actual run.
    assert!(receipt::decode(&receipt::encode(&value).unwrap()).is_ok());
}
