use morrow_core::{content::CardRecord, envelope, runtime::RenameRequest};
use std::{io::Read, process::ExitCode};
fn run() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    match args.as_slice() {
        [command] if command == "self-check" => {
            let card =
                CardRecord::new("sample-1", "example.unknown", 7, "Before", vec![0, 255, 42])?;
            let request = RenameRequest {
                operation_id: "sample-operation".into(),
                card_id: "sample-1".into(),
                expected_revision: 1,
                title: "After".into(),
            };
            let decoded = RenameRequest::decode(&request.encode()?)?;
            let edited = decoded.propose(&card)?;
            let restored = envelope::decode(&envelope::encode(&edited)?)?;
            assert_eq!(restored.summary().revision, 2);
            assert_eq!(restored.body(), card.body());
            println!(
                "PASS: Cap'n Proto command, pure edit, Protobuf + LZ4 round trip. No storage commit or plugin execution."
            );
        }
        [command, path] if command == "verify" => {
            let mut bytes = Vec::new();
            std::fs::File::open(path)?
                .take(envelope::MAX_CONTAINER_BYTES as u64 + 1)
                .read_to_end(&mut bytes)?;
            let card = envelope::decode(&bytes)?;
            println!(
                "Valid card container: revision={}, format={}",
                card.summary().revision,
                card.summary().format_version
            );
        }
        _ => return Err("usage: morrow-core-check self-check | verify <container>".into()),
    }
    Ok(())
}
fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("{e}");
            ExitCode::FAILURE
        }
    }
}
