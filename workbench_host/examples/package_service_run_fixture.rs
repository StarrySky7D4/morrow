//! Test-only WAT package: two exact authenticated service requests and replies.
//! The second argument binds the fixture to a config-created namespace; no guest
//! code dynamically parses arbitrary traffic and no SDK artifact is repackaged.
use morrow_core::{
    io::Header,
    plugin_package::{
        Package,
        io::{self, IoCapability},
    },
    service::{self, Invocation, Reply, Request, Response},
    service_record::{self, Policy},
};
use morrow_plugin_runtime::service_content::scope_digest;
use std::{collections::BTreeSet, io::Write, path::PathBuf};

const ID: &str = "org.example.workbench.application.service";
const SERVICE: &str = "application.service";
const HANDLER: &str = "application.serve";
const FIRST_KEY: &str = "application-service-before";
const SECOND_KEY: &str = "application-service-after";

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if !(1..=2).contains(&args.len()) {
        return Err("usage: output.mplugin [namespace-hex]".into());
    }
    let output = PathBuf::from(&args[0]);
    let namespace = if let Some(hex) = args.get(1) {
        if hex.len() != 64 || !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err("namespace must be 32 hex-encoded bytes".into());
        }
        let mut bytes = [0u8; 32];
        for (i, byte) in bytes.iter_mut().enumerate() {
            *byte = u8::from_str_radix(&hex[2 * i..2 * i + 2], 16)?;
        }
        if bytes == [0; 32] {
            return Err("namespace cannot be zero".into());
        }
        bytes
    } else {
        [19; 32]
    };
    let package = service_package(namespace, if args.len() == 2 { "1.0.1" } else { "1.0.0" });
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(output)?;
    file.write_all(package.archive())?;
    file.sync_all()?;
    Ok(())
}
fn policy(namespace: [u8; 32]) -> Policy {
    Policy {
        namespace,
        retention_ms: 30_000,
    }
}
fn expected(namespace: [u8; 32], key: &str, body: &[u8]) -> Request {
    let invocation = Invocation {
        service: SERVICE.into(),
        handler: HANDLER.into(),
        principal: "alice".into(),
        method: "POST".into(),
        target: "/api".into(),
        headers: vec![Header {
            name: "morrow-content-scope".into(),
            value: scope_digest(&[])
                .unwrap()
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect::<String>()
                .into_bytes(),
        }],
        body: body.to_vec(),
    };
    Request::encode(
        service_record::call_id(&policy(namespace), key, &invocation).unwrap(),
        &invocation,
    )
    .unwrap()
}
fn service_package(namespace: [u8; 32], version: &str) -> Package {
    let first = expected(namespace, FIRST_KEY, b"before");
    let second = expected(namespace, SECOND_KEY, b"after");
    let response = |request: &Request, body: &[u8]| {
        Response::encode(
            request,
            &Reply {
                status: 202,
                headers: vec![],
                body: body.to_vec(),
            },
        )
        .unwrap()
    };
    let first_reply = response(&first, b"executed-before");
    let second_reply = response(&second, b"executed-after");
    let quoted = |bytes: &[u8]| {
        bytes
            .iter()
            .map(|byte| format!("\\{byte:02x}"))
            .collect::<String>()
    };
    // Match complete authenticated request frames, including distinct durable call IDs.
    let wasm = wat::parse_str(format!(
        r#"(module
      (import "morrow_task_v1" "read_input" (func $read (param i32 i32) (result i32)))
      (import "morrow_task_v1" "complete" (func $done (param i32 i32) (result i32)))
      (memory (export "memory") 5)
      (data (i32.const 131072) "{first}") (data (i32.const 163840) "{second}")
      (data (i32.const 196608) "{first_reply}") (data (i32.const 229376) "{second_reply}")
      (func $matches (param $base i32) (param $size i32) (param $actual i32) (result i32)
        (local $i i32)
        local.get $actual local.get $size i32.ne if i32.const 0 return end
        (loop $check
          local.get $i i32.load8_u local.get $base local.get $i i32.add i32.load8_u
          i32.ne if i32.const 0 return end
          local.get $i i32.const 1 i32.add local.tee $i local.get $size i32.lt_u br_if $check)
        i32.const 1)
      (func (export "morrow_run") (result i32) (local $size i32)
        i32.const 0 i32.const 131072 call $read local.set $size
        i32.const 131072 i32.const {first_size} local.get $size call $matches
        if i32.const 196608 i32.const {first_reply_size} call $done drop
        else
          i32.const 163840 i32.const {second_size} local.get $size call $matches
          i32.eqz if unreachable end
          i32.const 229376 i32.const {second_reply_size} call $done drop
        end i32.const 0))"#,
        first = quoted(first.bytes()),
        second = quoted(second.bytes()),
        first_reply = quoted(&first_reply),
        second_reply = quoted(&second_reply),
        first_size = first.bytes().len(),
        second_size = second.bytes().len(),
        first_reply_size = first_reply.len(),
        second_reply_size = second_reply.len(),
    ))
    .unwrap();
    let mut manifest = Package::manifest_for_task(ID, version, &wasm, vec![]);
    let mut declaration = io::declaration(caps().into_iter().collect(), vec![HANDLER.into()]);
    declaration.service_schema_sha256 = service::schema_digest().to_vec();
    let limits = declaration.budget.as_mut().unwrap();
    limits.max_resources = 8;
    limits.max_jobs = 4;
    limits.max_job_bytes = 1024 * 1024;
    limits.max_bytes = 4 * 1024 * 1024;
    declaration.service_run = Some(io::proto::ServiceRunProfile {
        schema_version: io::SERVICE_RUN_VERSION,
        max_duration_ms: 120_000,
        budget: Some(io::proto::ServiceRunBudget {
            schema_version: io::SERVICE_RUN_BUDGET_VERSION,
            max_jobs: 64,
            max_bytes: 4 * 1024 * 1024,
        }),
    });
    manifest.required_features.extend([
        io::FEATURE.into(),
        io::SERVICE_RUN_FEATURE.into(),
        io::SERVICE_RUN_BUDGET_FEATURE.into(),
    ]);
    manifest.io_declaration = Some(declaration);
    Package::build(manifest, &wasm).unwrap()
}
fn caps() -> BTreeSet<IoCapability> {
    BTreeSet::from([IoCapability::HttpListen, IoCapability::HttpPublish])
}
