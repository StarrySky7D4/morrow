//! Exact old archives/modules, never rebuilt, resealed or repackaged.
use crate::Result;
use morrow_core::plugin_package::Package;
use sha2::{Digest, Sha256};
use std::{collections::BTreeSet, fs, path::Path};
const ROOT_PIN: &str = "cdda1d8fde36f984f95d193753eaf40b2f75db4bb119986ca1cc733710000266";
pub fn load(repo: &Path, language: &str, profile: &str) -> Result<Package> {
    if !matches!(language, "rust" | "c" | "cpp") || !matches!(profile, "task" | "transform") {
        return Err("unknown original guest".into());
    }
    let root = repo.join("sdk/compat/guest-v1-rc1");
    if root.symlink_metadata()?.file_type().is_symlink() {
        return Err("symlink baseline".into());
    }
    let manifest = fs::read(root.join("SHA256SUMS"))?;
    if format!("{:x}", Sha256::digest(&manifest)) != ROOT_PIN {
        return Err("frozen root differs; do not reseal".into());
    }
    let mut expected = BTreeSet::new();
    for lang in ["rust", "c", "cpp"] {
        for kind in ["task", "transform", "ui", "dependency"] {
            for ext in ["wasm", "mplugin"] {
                expected.insert(format!("{lang}-{kind}.{ext}"));
            }
        }
    }
    for n in [
        "rust-provider.wasm",
        "rust-provider.mplugin",
        "SOURCE_SHA256SUMS",
        "provenance.txt",
    ] {
        expected.insert(n.to_string());
    }
    for n in [
        "runtime.capnp",
        "content.proto",
        "task.capnp",
        "ui.capnp",
        "dependency_call.capnp",
        "version.txt",
        "task-version.txt",
        "ui-version.txt",
    ] {
        expected.insert(format!("contracts/{n}"));
    }
    let stem = format!("{language}-{profile}");
    let mut seen = BTreeSet::new();
    let mut archive = None;
    let mut module = None;
    for line in std::str::from_utf8(&manifest)?.lines() {
        let (hash, name) = line.split_once("  ").ok_or("baseline format")?;
        if !expected.contains(name) || !seen.insert(name.to_string()) {
            return Err("baseline path set".into());
        }
        let path = root.join(name);
        if path.symlink_metadata()?.file_type().is_symlink() {
            return Err("baseline file symlink".into());
        }
        let bytes = fs::read(path)?;
        if format!("{:x}", Sha256::digest(&bytes)) != hash {
            return Err(format!("original changed: {name}").into());
        }
        if name == format!("{stem}.mplugin") {
            archive = Some(bytes);
        } else if name == format!("{stem}.wasm") {
            module = Some(bytes);
        }
    }
    if seen != expected {
        return Err("incomplete original baseline".into());
    }
    let package = Package::decode(&archive.ok_or("missing original archive")?)?;
    if package.module() != module.ok_or("missing original wasm")? {
        return Err("archive/module mismatch".into());
    }
    if package.channel_declaration().is_some() {
        return Err("old task/transform unexpectedly declares channel".into());
    }
    Ok(package)
}
