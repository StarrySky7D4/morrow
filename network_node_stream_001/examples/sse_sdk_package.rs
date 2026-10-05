//! New typed SSE event guests; original channel is the sole required feature.
#[cfg(all(feature = "managed-channel", not(target_arch = "wasm32")))]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    use morrow_core::{
        channel,
        plugin_package::{MAX_MODULE_BYTES, Package, proto::TransformHandler},
    };
    use std::{
        fs::File,
        io::{Read, Write},
        path::Path,
    };
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.len() != 8 || args[0] != "pack" {
        return Err("sse_sdk_package pack MODULE OUTPUT ID VERSION sse-event-v1 1 DIGEST".into());
    }
    let digest = morrow_network_node_stream::managed_sse::schema_digest();
    if args[5] != "sse-event-v1" || args[6] != "1" || args[7] != hex(&digest) {
        return Err("unknown exact payload profile/version/digest".into());
    }
    let mut module = Vec::new();
    File::open(&args[1])?
        .take(MAX_MODULE_BYTES as u64 + 1)
        .read_to_end(&mut module)?;
    if module.len() > MAX_MODULE_BYTES {
        return Err("module too large".into());
    }
    let handler = TransformHandler {
        handler: "channel.sse.sdk".into(),
        input_type: "bytes".into(),
        output_type: "bytes".into(),
        max_input_bytes: 65,
        max_output_bytes: 64,
    };
    let mut manifest = Package::manifest_for_transform(&args[3], &args[4], &module, vec![handler]);
    manifest.required_features.push(channel::FEATURE.into());
    let mut declaration =
        channel::declaration(vec!["channel.sse.sdk".into()], vec![channel::Kind::Events]);
    declaration.budget = Some(
        channel::Budget {
            max_channels: 2,
            max_frame_bytes: 32768,
            max_bytes: 1048576,
            max_messages: 128,
            max_requests: 4096,
            max_duration_ms: 10000,
        }
        .to_proto(),
    );
    manifest.channel_declaration = Some(declaration);
    let execution = manifest.budget.as_mut().ok_or("missing execution budget")?;
    execution.fuel = 100_000_000;
    execution.memory_bytes = 16 * 1024 * 1024;
    execution.host_calls = 1024;
    let package = Package::build(manifest, &module)?;
    if !package.capabilities().is_empty() || package.io_declaration().is_some() {
        return Err("unexpected guest authority".into());
    }
    let target = Path::new(&args[2]);
    let parent = target
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let mut staged = tempfile::NamedTempFile::new_in(parent)?;
    staged.write_all(package.archive())?;
    staged.as_file().sync_all()?;
    staged.persist_noclobber(target)?;
    println!(
        "published={} sha256={} payload_profile={}",
        target.display(),
        hex(&package.digest()),
        hex(&digest)
    );
    Ok(())
}
#[cfg(all(feature = "managed-channel", not(target_arch = "wasm32")))]
fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
#[cfg(any(not(feature = "managed-channel"), target_arch = "wasm32"))]
fn main() {
    panic!("native managed-channel packer feature required")
}
