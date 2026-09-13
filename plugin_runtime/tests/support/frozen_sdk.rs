//! Read-only baseline loader shared by the actual old-binary execution tests.
use morrow_core::plugin_package::Package;
use sha2::{Digest, Sha256};
use std::{collections::BTreeSet, fs, path::PathBuf};

pub fn package(stem: &str) -> Package {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../sdk/compat/guest-v1-rc1");
    assert!(!root.symlink_metadata().unwrap().file_type().is_symlink());
    let sums = fs::read(root.join("SHA256SUMS")).expect("missing frozen baseline manifest");
    let pin = include_str!("../../../sdk/compat/guest-v1-rc1.sha256").trim();
    assert_eq!(
        format!("{:x}", Sha256::digest(&sums)),
        pin,
        "baseline root changed; never reseal to repair a compatibility failure"
    );
    let mut expected = BTreeSet::new();
    for language in ["rust", "c", "cpp"] {
        for profile in ["task", "transform", "ui", "dependency"] {
            for extension in ["wasm", "mplugin"] {
                expected.insert(format!("{language}-{profile}.{extension}"));
            }
        }
    }
    for name in [
        "rust-provider.wasm",
        "rust-provider.mplugin",
        "SOURCE_SHA256SUMS",
        "provenance.txt",
    ] {
        expected.insert(name.to_owned());
    }
    for name in [
        "runtime.capnp",
        "content.proto",
        "task.capnp",
        "ui.capnp",
        "dependency_call.capnp",
        "version.txt",
        "task-version.txt",
        "ui-version.txt",
    ] {
        expected.insert(format!("contracts/{name}"));
    }
    assert!(
        expected.contains(&format!("{stem}.mplugin")),
        "unknown baseline profile"
    );
    let mut seen = BTreeSet::new();
    let mut selected_archive = None;
    let mut selected_module = None;
    for line in std::str::from_utf8(&sums).unwrap().lines() {
        let (hash, name) = line.split_once("  ").expect("invalid baseline manifest");
        assert!(
            expected.contains(name),
            "unexpected or escaping baseline path"
        );
        assert!(seen.insert(name.to_owned()), "duplicate baseline entry");
        let file = root.join(name);
        assert!(!file.symlink_metadata().unwrap().file_type().is_symlink());
        let bytes = fs::read(file).unwrap();
        assert_eq!(
            format!("{:x}", Sha256::digest(&bytes)),
            hash,
            "frozen file changed: {name}"
        );
        if name == format!("{stem}.mplugin") {
            selected_archive = Some(bytes);
        } else if name == format!("{stem}.wasm") {
            selected_module = Some(bytes);
        }
    }
    assert_eq!(seen, expected, "incomplete frozen baseline");
    assert!(
        !root
            .join("contracts")
            .symlink_metadata()
            .unwrap()
            .file_type()
            .is_symlink()
    );
    // Execute the exact buffers whose hashes passed; do not reopen the files.
    let package = Package::decode(&selected_archive.expect("missing selected package"))
        .expect("current host rejected an original frozen package");
    assert_eq!(
        package.module(),
        selected_module.expect("missing selected module"),
        "package module differs from the frozen Wasm"
    );
    package
}
