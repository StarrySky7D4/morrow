//! Independent host-generated fixtures for guest SDK qualification.
use morrow_core::{
    attachment::AttachmentChunk,
    content::CardSummary,
    response::{Failure, Outcome, Response},
    runtime::{Command, ReadAttachment, RenameRequest},
    transaction::{Lookup, Receipt},
};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let dir = std::path::PathBuf::from(std::env::args().nth(1).ok_or("output directory")?);
    std::fs::create_dir_all(&dir)?;
    let receipt = Receipt {
        operation_id: "vector-op".into(),
        card_id: "legacy-123".into(),
        event_id: "event-1".into(),
        revision: u64::MAX,
        content_sha256: [42; 32],
    };
    let commands = [
        (
            "content-create",
            Command::CreateContent(morrow_core::runtime::CreateContent {
                operation_id: "vector-op".into(),
                card_id: "legacy-123".into(),
                type_id: "morrow.note".into(),
                format_version: 1,
                title: "消息 🪷".into(),
                body: vec![0, 255, 42],
            }),
        ),
        (
            "content-edit",
            Command::EditContent(morrow_core::content_change::ContentChange {
                operation_id: "vector-op".into(),
                card_id: "legacy-123".into(),
                expected_revision: u64::MAX - 1,
                title: "消息 🪷".into(),
                body: vec![0, 255, 42],
                preview_text: "preview".into(),
                attachments: None,
            }),
        ),
        (
            "content-read",
            Command::ReadContent(morrow_core::runtime::ReadContent {
                request_id: "vector-op".into(),
                card_id: "legacy-123".into(),
                expected_revision: u64::MAX,
                offset: 2,
                length: 3,
            }),
        ),
        (
            "rename",
            Command::Rename(RenameRequest {
                operation_id: "vector-op".into(),
                card_id: "legacy-123".into(),
                expected_revision: u64::MAX - 1,
                title: "消息 🪷".into(),
            }),
        ),
        (
            "summary",
            Command::ReadSummary {
                request_id: "vector-op".into(),
                card_id: "legacy-123".into(),
            },
        ),
        (
            "query",
            Command::QueryOperation {
                request_id: "vector-op".into(),
                card_id: "legacy-123".into(),
                operation_id: "vector-op".into(),
            },
        ),
        (
            "attachment",
            Command::ReadAttachment(ReadAttachment {
                request_id: "vector-op".into(),
                card_id: "legacy-123".into(),
                attachment_id: "asset-1".into(),
                expected_revision: u64::MAX,
                offset: 2,
                length: 3,
            }),
        ),
    ];
    for (name, value) in commands {
        std::fs::write(dir.join(format!("{name}-request.capnp")), value.encode()?)?;
    }
    let outcomes = [
        (
            "content-committed",
            Outcome::ContentCommitted(receipt.clone()),
        ),
        (
            "content",
            Outcome::ContentChunk(morrow_core::runtime::ContentChunk {
                card_id: "legacy-123".into(),
                revision: u64::MAX,
                offset: 2,
                total_length: 5,
                body_sha256: [9; 32],
                bytes: vec![0, 255, 42],
            }),
        ),
        ("renamed", Outcome::Renamed(receipt.clone())),
        (
            "summary",
            Outcome::Summary(CardSummary {
                id: "legacy-123".into(),
                type_id: "morrow.note".into(),
                format_version: 1,
                revision: u64::MAX,
                title: "消息 🪷".into(),
                preview_text: "preview".into(),
            }),
        ),
        ("denied", Outcome::Rejected(Failure::Denied)),
        (
            "absent",
            Outcome::OperationResult {
                card_id: "legacy-123".into(),
                operation_id: "vector-op".into(),
                result: Lookup::Absent,
            },
        ),
        (
            "committed",
            Outcome::OperationResult {
                card_id: "legacy-123".into(),
                operation_id: "vector-op".into(),
                result: Lookup::Committed(receipt),
            },
        ),
        (
            "attachment",
            Outcome::AttachmentChunk(AttachmentChunk {
                card_id: "legacy-123".into(),
                attachment_id: "asset-1".into(),
                revision: u64::MAX,
                offset: 2,
                total_length: 5,
                content_sha256: [42; 32],
                bytes: vec![0, 255, 1],
            }),
        ),
    ];
    for (name, outcome) in outcomes {
        std::fs::write(
            dir.join(format!("{name}-reply.capnp")),
            Response {
                request_id: "vector-op".into(),
                outcome,
            }
            .encode()?,
        )?;
    }
    println!("Wrote fifteen independent host codec fixtures");
    Ok(())
}
