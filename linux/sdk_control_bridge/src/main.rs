//! Headless standalone fixture, not a production Workbench entry point.
use morrow_linux_sdk_control_bridge::{
    Result, driver,
    transport::{Adapter, channel_callback, pause_callback, runtime_callback, transform_callback},
};
use morrow_linux_supervisor_foundation::take_controller_fixture_transport;
use morrow_plugin_sdk as sdk;
use std::{ffi::c_void, path::Path};
fn main() {
    let a: Vec<_> = std::env::args().skip(1).collect();
    let result = match a.first().map(String::as_str) {
        Some("supervisor") => driver::supervisor(Path::new(&a[1]), Path::new(&a[2]), &a[3]),
        Some("controller") => controller(&a[1]),
        _ => Err("unknown isolated fixture role".into()),
    };
    if let Err(e) = result {
        eprintln!("SDK-FIXTURE-FAILED: {e}");
        std::process::exit(101);
    }
}
fn sdk_result<T, E: std::fmt::Debug>(r: std::result::Result<T, E>) -> Result<T> {
    r.map_err(|e| format!("SDK contract: {e:?}").into())
}
fn controller(mode: &str) -> Result<()> {
    let peer = unsafe { take_controller_fixture_transport() }?;
    let mut adapter = Box::new(Adapter::new(peer));
    let directory = adapter.initial_directory()?;
    let directory = sdk_result(sdk::channel::Directory::decode(&directory))?;
    if directory.channels.len() != 1 {
        return Err("exact original bound endpoint required".into());
    }
    let endpoint = directory.channels[0].clone();
    let context = (adapter.as_mut() as *mut Adapter).cast::<c_void>();
    // Box address remains fixed and alive until every client/host below is dropped.
    // All callbacks are synchronous and serial on this isolated C's main thread.
    let runtime_host = sdk::HostV1 {
        abi_version: 1,
        struct_size: size_of::<sdk::HostV1>() as u32,
        context,
        exchange: Some(runtime_callback),
    };
    let transform_host = sdk::HostV1 {
        exchange: Some(transform_callback),
        ..runtime_host
    };
    let pause_host = sdk::HostV1 {
        exchange: Some(pause_callback),
        ..runtime_host
    };
    let channel_host = sdk::channel::transport::HostV1 {
        abi_version: 1,
        struct_size: size_of::<sdk::channel::transport::HostV1>() as u32,
        context,
        call: Some(channel_callback),
    };
    let mut runtime = sdk_result(unsafe { sdk::Client::from_host(&runtime_host) })?;
    let mut transform = sdk_result(unsafe { sdk::Client::from_host(&transform_host) })?;
    let mut channel =
        sdk_result(unsafe { sdk::channel::transport::Client::from_host(&channel_host) })?;
    let request = sdk::protocol::Request {
        request_id: "original-rename".into(),
        card_id: "bridge-card".into(),
        action: sdk::protocol::Action::Rename {
            revision: 1,
            title: "renamed through actual original guest".into(),
        },
    };
    let bytes = sdk_result(request.encode())?;
    let reply = sdk_result(runtime.exchange(&bytes))?;
    let first_revision = match sdk_result(request.decode_reply(&reply))? {
        sdk::protocol::Reply::Renamed(receipt) => receipt.revision,
        _ => return Err("actual original task did not commit".into()),
    };
    let input = b"actual original transform \0\xff".to_vec();
    let invocation = morrow_core::task::Invocation::new_transform(
        "bridge-transform",
        morrow_core::task::Transform {
            handler: "bytes.reverse".into(),
            input_type: "bytes".into(),
            output_type: "bytes".into(),
            input: input.clone(),
        },
    )?;
    let output = sdk_result(transform.exchange(invocation.bytes()))?;
    let mut expected = input;
    expected.reverse();
    if output != expected {
        return Err("original transform output differs".into());
    }
    let mut call = 0u64;
    let mut request_for = |action| {
        call += 1;
        let mut id = [0u8; 32];
        id[..8].copy_from_slice(&call.to_le_bytes());
        id[31] = 0x6b;
        sdk::channel::Request {
            call_id: id,
            reference: endpoint.reference,
            source_epoch: endpoint.source_epoch,
            action,
        }
    };
    let began = std::time::Instant::now();
    let frame = loop {
        let r = request_for(sdk::channel::Action::Receive {
            last_acked: 0,
            credit_bytes: 32768,
        });
        let reply = sdk_result(channel.call(&r))?;
        if reply.status == sdk::channel::Status::Frame {
            break reply.frame.ok_or("no actual producer frame")?;
        }
        if reply.status != sdk::channel::Status::Idle
            || began.elapsed() >= std::time::Duration::from_secs(1)
        {
            return Err(format!(
                "bounded actual producer readiness failed: {:?};reclaimed={}",
                reply.status, reply.resource_reclaimed
            )
            .into());
        }
        std::thread::sleep(std::time::Duration::from_millis(1));
    };
    if frame.bytes != b"actual broker producer bytes" || !frame.cursor.is_empty() {
        return Err("actual broker bytes/cursor mismatch".into());
    }
    let digest = sdk_result(frame.digest())?;
    let bad = request_for(sdk::channel::Action::Ack {
        sequence: frame.sequence,
        frame_sha256: digest,
        cursor: b"wrong cursor".to_vec(),
    });
    let response = sdk_result(channel.call(&bad))?;
    if response.status != sdk::channel::Status::Invalid
        || response.last_acked != 0
        || response.resource_reclaimed
    {
        return Err("invalid ACK advanced/reclaimed".into());
    }
    let ack = request_for(sdk::channel::Action::Ack {
        sequence: frame.sequence,
        frame_sha256: digest,
        cursor: frame.cursor,
    });
    let response = sdk_result(channel.call(&ack))?;
    if response.status != sdk::channel::Status::Acked
        || response.last_acked != frame.sequence
        || response.resource_reclaimed
    {
        return Err("ACK is not cursor-only receipt".into());
    }
    let send = request_for(sdk::channel::Action::Send {
        sequence: 1,
        bytes: b"actual SDK send".to_vec(),
    });
    let response = sdk_result(channel.call(&send))?;
    if response.status != sdk::channel::Status::Accepted
        || response.accepted_sequence != 1
        || response.resource_reclaimed
    {
        return Err("send admission differs".into());
    }
    let query = request_for(sdk::channel::Action::Query);
    let response = sdk_result(channel.call(&query))?;
    if response.resource_reclaimed {
        return Err("live original producer falsely reclaimed".into());
    }
    println!(
        "actual-SDK-client=true;original-task-commit=true;original-transform-output=true;wrong-ACK-denied=true;cursor-ACK=true;send-Accepted-not-success=true;live-producer-not-reclaimed=true;no-auto-replay=true"
    );
    if mode == "sdk-close" {
        let close = request_for(sdk::channel::Action::Close);
        let mut response = sdk_result(channel.call(&close))?;
        println!(
            "actual-first-SDK-Close-status={:?};resource-reclaimed={};Pending-not-inferred=true",
            response.status, response.resource_reclaimed
        );
        let close_started = std::time::Instant::now();
        loop {
            if response.status == sdk::channel::Status::Closed && response.resource_reclaimed {
                break;
            }
            if !matches!(
                response.status,
                sdk::channel::Status::Closed | sdk::channel::Status::ClosingUnconfirmed
            ) || close_started.elapsed() >= std::time::Duration::from_secs(1)
            {
                return Err(
                    format!("bounded SDK Close not reclaimed: {:?}", response.status).into(),
                );
            }
            let query = request_for(sdk::channel::Action::Query);
            response = sdk_result(channel.call(&query))?;
        }
        if response.last_acked != frame.sequence
            || response.frame.is_some()
            || response.accepted_sequence != 0
        {
            return Err("Close response advanced original channel".into());
        }
        for action in [
            sdk::channel::Action::Receive {
                last_acked: frame.sequence,
                credit_bytes: 32768,
            },
            sdk::channel::Action::Send {
                sequence: 2,
                bytes: b"must not be delivered after Close".to_vec(),
            },
        ] {
            let rejected = request_for(action);
            let response = sdk_result(channel.call(&rejected))?;
            if response.status != sdk::channel::Status::Closed
                || response.frame.is_some()
                || response.accepted_sequence != 0
                || response.last_acked != frame.sequence
            {
                return Err(format!(
                    "closed-channel fresh data not precisely refused: {:?}",
                    response.status
                )
                .into());
            }
        }
        let query = request_for(sdk::channel::Action::Query);
        let response = sdk_result(channel.call(&query))?;
        if response.status != sdk::channel::Status::Closed
            || !response.resource_reclaimed
            || response.last_acked != frame.sequence
            || response.frame.is_some()
        {
            return Err("post-refusal Query lost Closed receipt".into());
        }
        let after = sdk::protocol::Request {
            request_id: "after-sdk-close-original-rename".into(),
            card_id: "bridge-card".into(),
            action: sdk::protocol::Action::Rename {
                revision: first_revision,
                title: "same original instance still live after channel Close".into(),
            },
        };
        let reply = sdk_result(runtime.exchange(&sdk_result(after.encode())?))?;
        match sdk_result(after.decode_reply(&reply))? {
            sdk::protocol::Reply::Renamed(receipt) if receipt.revision == first_revision + 1 => (),
            _ => return Err("same original task failed after channel Close".into()),
        }
        let input = b"fresh original transform after SDK Close\0\xff".to_vec();
        let invocation = morrow_core::task::Invocation::new_transform(
            "after-sdk-close-transform",
            morrow_core::task::Transform {
                handler: "bytes.reverse".into(),
                input_type: "bytes".into(),
                output_type: "bytes".into(),
                input: input.clone(),
            },
        )?;
        let output = sdk_result(transform.exchange(invocation.bytes()))?;
        let mut expected = input;
        expected.reverse();
        if output != expected {
            return Err("same original transform failed after channel Close".into());
        }
        println!(
            "actual-SDK-Close-Query=true;closed-channel-Receive-Send-refused=true;original-task-after-close-commit=true;original-transform-after-close-output=true;original-grant-budget-unchanged=true;no-auto-replay=true"
        );
    }
    drop(channel);
    drop(transform);
    drop(runtime);
    if mode == "held-worker" {
        let mut pause = sdk_result(unsafe { sdk::Client::from_host(&pause_host) })?;
        let _ = pause.exchange(&[1]);
        return Err("C should be exactly collected while real worker held".into());
    }
    if mode == "malformed-call" {
        adapter.malformed_call_fixture()?;
        // G keeps this real original C alive while S's authenticated-error
        // cleanup retires the wire. No business/heartbeat retry after retirement.
        std::thread::park_timeout(std::time::Duration::from_secs(4));
        return Err("C not exactly collected after error cleanup".into());
    }
    adapter.heartbeat_until_collected()
}
