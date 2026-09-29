//! Generates deterministic request/reply byte vectors using only the in-memory fake.
#[path = "../tests/common/mod.rs"]
mod common;
use common::*;
use morrow_agent_host_contract::{agent_host_capnp as wire, fake::*, *};
use std::{env, fs, path::PathBuf};
fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = env::args_os().skip(1).collect();
    if args.len() != 2 || args[0] != "--output" {
        return Err("required: --output NEW_DIRECTORY".into());
    }
    let output = PathBuf::from(&args[1]);
    fs::create_dir(&output)?; // Existing successful output is never reused or removed.
    let mut host = fixture();
    let mut entries = Vec::new();
    let mut capture = |name: &str,
                       input: Vec<u8>,
                       expected: &str,
                       host: &mut FakeHost|
     -> Result<(), Box<dyn std::error::Error>> {
        let reply = host.exchange(&input)?;
        let input_name = format!("{name}.request.capnp");
        let output_name = format!("{name}.reply.capnp");
        fs::write(output.join(&input_name), &input)?;
        fs::write(output.join(&output_name), &reply)?;
        entries.push(format!("{{\"name\":\"{name}\",\"expected\":\"{expected}\",\"request\":\"{input_name}\",\"request_sha256\":\"{}\",\"reply\":\"{output_name}\",\"reply_sha256\":\"{}\"}}", hex(&digest(&input)), hex(&digest(&reply))));
        Ok(())
    };
    capture("00-before-hello", open(2, b"abc"), "denied", &mut host)?;
    capture(
        "01-hello",
        hello(&FixtureIdentity::default(), 1),
        "qualification-capabilities",
        &mut host,
    )?;
    capture("02-open", open(2, b"abc"), "prepared", &mut host)?;
    capture(
        "03-read-before-commit",
        read(3, 0, 5),
        "conflict",
        &mut host,
    )?;
    capture("04-write", chunk(4, 0, b"abc"), "prepared", &mut host)?;
    capture(
        "05-commit",
        stream(5, |mut s| s.set_commit_request(())),
        "committed",
        &mut host,
    )?;
    capture("06-first-chunk", read(6, 0, 5), "streaming", &mut host)?;
    capture(
        "07-final-chunk",
        read(7, 5, 4096),
        "eof-not-model-complete",
        &mut host,
    )?;
    capture(
        "08-append",
        append(8, 1, 9007199254740993, 0, b"opaque-event"),
        "tail-1",
        &mut host,
    )?;
    capture(
        "09-append-retry",
        append(8, 1, 9007199254740993, 0, b"opaque-event"),
        "original-tail-1",
        &mut host,
    )?;
    capture(
        "10-append-conflict",
        append(8, 1, 9007199254740993, 0, b"changed"),
        "conflict",
        &mut host,
    )?;
    capture(
        "11-propose",
        propose(11),
        "proposed-no-authority",
        &mut host,
    )?;
    capture("12-forged-permit", claim(12, &[0; 32]), "denied", &mut host)?;
    let permit = host.approve("operation-1")?;
    capture("13-claim", claim(13, &permit), "execute-once", &mut host)?;
    capture(
        "14-claim-retry",
        claim(14, &permit),
        "execute-false",
        &mut host,
    )?;
    capture(
        "15-report-unknown",
        report(15, ToolState::Unknown),
        "unknown-retained",
        &mut host,
    )?;
    capture(
        "16-drain",
        request(16, |f| f.init_native_session().set_drain(())),
        "closing-unconfirmed",
        &mut host,
    )?;
    capture("17-drained-claim", claim(17, &permit), "denied", &mut host)?;
    capture(
        "18-observe-exit",
        request(18, |f| f.init_native_session().set_observe_exit(())),
        "unavailable",
        &mut host,
    )?;
    let mut invalid = frame(19, "fixture-session", 1);
    invalid.get_root::<wire::frame::Builder>()?.set_major(99);
    let invalid = capnp::serialize::write_message_to_words(&invalid);
    assert_eq!(decode(&invalid).err().unwrap(), Error::UnsupportedVersion);
    fs::write(output.join("unknown-version.request.capnp"), &invalid)?;
    let manifest = format!(
        "{{\"format\":1,\"qualification_only\":true,\"schema_sha256\":\"{}\",\"vectors\":[{}],\"rejections\":[{{\"request\":\"unknown-version.request.capnp\",\"sha256\":\"{}\",\"error\":\"UnsupportedVersion\"}}]}}\n",
        hex(&SCHEMA_DIGEST),
        entries.join(","),
        hex(&digest(&invalid))
    );
    fs::write(output.join("manifest.json"), manifest)?; // Complete marker is always last.
    println!(
        "Generated {} request/reply vectors plus unknown-version rejection",
        entries.len()
    );
    Ok(())
}
