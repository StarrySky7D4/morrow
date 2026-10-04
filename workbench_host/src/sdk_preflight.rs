//! Read-only package diagnostics for this exact compiled host.
//!
//! Preparation decodes the bounded archive and statically validates its module.
//! It creates no Wasm Store, instance, grants, owner, database or installation,
//! and never invokes a guest. A prepared package is not a usable host route or
//! production/platform qualification. Runtime allocation and execution can fail.
use morrow_core::plugin_package::{self, Package, catalog};
use morrow_plugin_runtime::{Limits, package::PreparedPackage};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{ffi::OsString, fs, path::Path};

pub const COMMAND: &str = "--sdk-preflight";
/// Includes the single trailing newline emitted by the CLI.
pub const MAX_OUTPUT_BYTES: usize = 16 * 1024;

fn hard_byte_limits() -> Value {
    json!({
        "archive": plugin_package::MAX_PACKAGE_BYTES,
        "module": morrow_plugin_runtime::MAX_MODULE_BYTES
    })
}

/// Independently versioned optional advertisement; discovery grants nothing.
pub fn advertisement() -> Value {
    json!({
        "schema_version": 1,
        "command": COMMAND,
        "read_only": true,
        "preparation": "static_only",
        "max_output_bytes": MAX_OUTPUT_BYTES,
        "authority": "none",
        "hard_byte_limits": hard_byte_limits()
    })
}

fn limits(limits: Limits) -> Value {
    json!({"fuel": limits.fuel, "memory_bytes": limits.memory_bytes, "host_calls": limits.host_calls})
}

fn response(phase: &str) -> Value {
    json!({
        "schema_version": 1,
        "diagnostic": "package_preflight",
        "host_version": env!("CARGO_PKG_VERSION"),
        "platform": {"os": std::env::consts::OS, "arch": std::env::consts::ARCH},
        "backend": "wasmi",
        "status": "rejected",
        "phase": phase,
        "preparation": "static_only",
        "authority": "none",
        "grants_created": 0,
        "guest_executed": false,
        "installed": false,
        "production_qualified": false,
        "routes_qualified": false,
        "package": null,
        "host_limits": limits(Limits::default()),
        "effective_limits": null,
        "hard_byte_limits": hard_byte_limits(),
        "error": null
    })
}

fn rejected(mut value: Value, code: &str, message: impl Into<String>) -> Value {
    value["error"] = json!({"code": code, "message": message.into()});
    value
}

fn regular_file(metadata: &fs::Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
        if metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
            return false;
        }
    }
    metadata.file_type().is_file()
}

fn package_identity(package: &Package) -> Value {
    let manifest = package.manifest();
    let budget = manifest.budget.as_ref().expect("validated package budget");
    json!({
        "id": manifest.package_id,
        "version": manifest.package_version,
        "archive_sha256": format!("{:x}", Sha256::digest(package.archive())),
        "module_sha256": format!("{:x}", Sha256::digest(package.module())),
        "archive_bytes": package.archive().len(),
        "module_bytes": package.module().len(),
        "manifest_schema_version": manifest.schema_version,
        "guest_abi_version": manifest.guest_abi_version,
        "declared_limits": {"fuel": budget.fuel, "memory_bytes": budget.memory_bytes, "host_calls": budget.host_calls}
    })
}

fn inspect(path: &Path) -> Value {
    let mut value = response("package_read_decode");
    // Reject ordinary FIFO/device/directory/symlink inputs without opening them.
    // This path-based precheck is not a hostile-filesystem sandbox: a concurrent
    // replacement or ancestor change can race catalog::read_file. Callers must
    // bound the child process lifetime and verify their selected archive bytes.
    match fs::symlink_metadata(path) {
        Ok(metadata) if regular_file(&metadata) => {}
        Ok(_) => return rejected(value, "invalid_file_type", "package must be a regular file"),
        Err(_) => return rejected(value, "package_unavailable", "package metadata unavailable"),
    }
    let package = match catalog::read_file(path) {
        Ok(package) => package,
        Err(error) => return rejected(value, "package_rejected", format!("{error}")),
    };
    value["package"] = package_identity(&package);
    value["phase"] = json!("static_preparation");
    // This is the original preparation path; never connect, instantiate or run.
    match PreparedPackage::new(package, Limits::default()) {
        Ok(prepared) => {
            value["status"] = json!("prepared");
            value["effective_limits"] = limits(prepared.limits());
            value
        }
        Err(fault) => rejected(value, "preparation_rejected", format!("{fault:?}")),
    }
}

pub struct Diagnostic {
    /// Exactly one JSON value; the CLI adds a newline.
    pub json: String,
    /// Zero is static preparation only; two is any rejected diagnostic.
    pub exit_code: u8,
}

/// Arguments after COMMAND only. Validate arity before touching the package.
/// OsString preserves ordinary Unicode and native non-UTF-8 package paths.
pub fn command(args: impl IntoIterator<Item = OsString>) -> Diagnostic {
    let mut args = args.into_iter();
    let first = args.next();
    let value = match (first, args.next()) {
        (Some(path), None) if !path.is_empty() => inspect(Path::new(&path)),
        _ => rejected(
            response("arguments"),
            "invalid_arguments",
            "usage: morrow-workbench-host --sdk-preflight PACKAGE",
        ),
    };
    let exit_code = if value["status"] == "prepared" { 0 } else { 2 };
    let json = value.to_string();
    if json.len() < MAX_OUTPUT_BYTES {
        Diagnostic { json, exit_code }
    } else {
        Diagnostic {
            json: rejected(
                response("static_preparation"),
                "output_limit",
                "diagnostic exceeded its output limit",
            )
            .to_string(),
            exit_code: 2,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn advertised_limits_and_no_authority_match_response() {
        let ad = advertisement();
        let result = command([]);
        let value: Value = serde_json::from_str(&result.json).unwrap();
        assert_eq!(result.exit_code, 2);
        assert_eq!(value["phase"], "arguments");
        assert_eq!(value["hard_byte_limits"], ad["hard_byte_limits"]);
        assert_eq!(value["preparation"], ad["preparation"]);
        assert_eq!(value["authority"], "none");
        assert_eq!(value["grants_created"], 0);
        assert_eq!(value["package"], Value::Null);
        assert_eq!(value["effective_limits"], Value::Null);
        assert!(result.json.len() < MAX_OUTPUT_BYTES);
        for field in [
            "guest_executed",
            "installed",
            "production_qualified",
            "routes_qualified",
        ] {
            assert_eq!(value[field], false);
        }
    }
}
