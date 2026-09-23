//! Portable persistent transaction contract, shared by native and future Web storage.
use crate::{
    Error, Result,
    content::{Attachment, CardRecord},
    envelope, identity,
    runtime::RenameRequest,
};
use prost::Message;
use sha2::{Digest, Sha256};
pub mod proto {
    include!(concat!(env!("OUT_DIR"), "/morrow.transaction.v1.rs"));
}
const MAGIC: &[u8; 8] = b"MORROWT1";
pub const MAX_EVENT_BYTES: usize = 16 * 1024 * 1024;
pub const MAX_TASK_EVIDENCE: usize = 16;
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Receipt {
    pub operation_id: String,
    pub card_id: String,
    pub revision: u64,
    pub content_sha256: [u8; 32],
    pub event_id: String,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Lookup {
    Committed(Receipt),
    // Absence is a snapshot, never proof that an in-flight operation cannot commit.
    Absent,
}
pub fn create_command(operation_id: &str, card: &CardRecord) -> Result<Vec<u8>> {
    identity(operation_id)?;
    Ok(proto::Command {
        schema_version: 1,
        operation_id: operation_id.into(),
        action: Some(proto::command::Action::CreateCard(card.encode())),
    }
    .encode_to_vec())
}
pub fn content_command(change: &crate::content_change::ContentChange) -> Result<Vec<u8>> {
    change.validate()?;
    Ok(proto::Command {
        schema_version: 1,
        operation_id: change.operation_id.clone(),
        action: Some(proto::command::Action::SetContent(proto::SetContent {
            card_id: change.card_id.clone(),
            expected_revision: change.expected_revision,
            title: change.title.clone(),
            body: change.body.clone(),
            preview_text: change.preview_text.clone(),
            attachments: change
                .attachments
                .as_ref()
                .map(|items| proto::AttachmentList {
                    items: items
                        .iter()
                        .map(|v| proto::AttachmentRef {
                            id: v.id.clone(),
                            display_name: v.display_name.clone(),
                            media_type: v.media_type.clone(),
                            byte_length: v.byte_length,
                            sha256: v.sha256.to_vec(),
                        })
                        .collect(),
                }),
        })),
    }
    .encode_to_vec())
}
pub fn migration_command(change: &crate::content_migration::ContentMigration) -> Result<Vec<u8>> {
    change.validate()?;
    let command = proto::Command {
        schema_version: 2,
        operation_id: change.operation_id.clone(),
        action: Some(proto::command::Action::MigrateContent(
            proto::MigrateContent {
                source_card: change.source_card.clone(),
                target_format_version: change.target_format_version,
                body: change.body.clone(),
                preview_text: change.preview_text.clone(),
            },
        )),
    };
    if command.encoded_len() > MAX_EVENT_BYTES {
        return Err(Error::Limit);
    }
    Ok(command.encode_to_vec())
}
pub fn versioned_content_command(
    change: &crate::versioned_content_change::VersionedContentChange,
) -> Result<Vec<u8>> {
    change.validate()?;
    let command = proto::Command {
        schema_version: 2,
        operation_id: change.operation_id.clone(),
        action: Some(proto::command::Action::SetVersionedContent(
            proto::SetVersionedContent {
                source_card: change.source_card.clone(),
                title: change.title.clone(),
                body: change.body.clone(),
                preview_text: change.preview_text.clone(),
                attachments: change
                    .attachments
                    .as_ref()
                    .map(|items| proto::AttachmentList {
                        items: items
                            .iter()
                            .map(|v| proto::AttachmentRef {
                                id: v.id.clone(),
                                display_name: v.display_name.clone(),
                                media_type: v.media_type.clone(),
                                byte_length: v.byte_length,
                                sha256: v.sha256.to_vec(),
                            })
                            .collect(),
                    }),
            },
        )),
    };
    if command.encoded_len() > MAX_EVENT_BYTES {
        return Err(Error::Limit);
    }
    Ok(command.encode_to_vec())
}

fn versioned_content_value(
    operation_id: &str,
    value: &proto::SetVersionedContent,
) -> Result<crate::versioned_content_change::VersionedContentChange> {
    Ok(crate::versioned_content_change::VersionedContentChange {
        operation_id: operation_id.into(),
        source_card: value.source_card.clone(),
        title: value.title.clone(),
        body: value.body.clone(),
        preview_text: value.preview_text.clone(),
        attachments: value
            .attachments
            .as_ref()
            .map(|v| attachment_values(&v.items))
            .transpose()?,
    })
}
fn migration_value(
    operation_id: &str,
    value: &proto::MigrateContent,
) -> crate::content_migration::ContentMigration {
    crate::content_migration::ContentMigration {
        operation_id: operation_id.into(),
        source_card: value.source_card.clone(),
        target_format_version: value.target_format_version,
        body: value.body.clone(),
        preview_text: value.preview_text.clone(),
    }
}
fn content_value(
    operation_id: &str,
    value: &proto::SetContent,
) -> Result<crate::content_change::ContentChange> {
    Ok(crate::content_change::ContentChange {
        operation_id: operation_id.into(),
        card_id: value.card_id.clone(),
        expected_revision: value.expected_revision,
        title: value.title.clone(),
        body: value.body.clone(),
        preview_text: value.preview_text.clone(),
        attachments: value
            .attachments
            .as_ref()
            .map(|v| attachment_values(&v.items))
            .transpose()?,
    })
}
pub fn rename_command(request: &RenameRequest) -> Result<Vec<u8>> {
    request.validate()?;
    Ok(proto::Command {
        schema_version: 1,
        operation_id: request.operation_id.clone(),
        action: Some(proto::command::Action::Rename(proto::Rename {
            card_id: request.card_id.clone(),
            expected_revision: request.expected_revision,
            title: request.title.clone(),
        })),
    }
    .encode_to_vec())
}
pub fn set_attachments_command(
    operation_id: &str,
    card_id: &str,
    expected_revision: u64,
    attachments: &[Attachment],
) -> Result<Vec<u8>> {
    identity(operation_id)?;
    identity(card_id)?;
    if expected_revision == 0 || attachments.len() > 1024 {
        return Err(Error::Invalid("attachment command"));
    }
    let mut ids = std::collections::BTreeSet::new();
    for item in attachments {
        item.validate()?;
        if !ids.insert(&item.id) {
            return Err(Error::Invalid("duplicate attachment id"));
        }
    }
    Ok(proto::Command {
        schema_version: 1,
        operation_id: operation_id.into(),
        action: Some(proto::command::Action::SetAttachments(
            proto::SetAttachments {
                card_id: card_id.into(),
                expected_revision,
                attachments: attachments
                    .iter()
                    .map(|v| proto::AttachmentRef {
                        id: v.id.clone(),
                        display_name: v.display_name.clone(),
                        media_type: v.media_type.clone(),
                        byte_length: v.byte_length,
                        sha256: v.sha256.to_vec(),
                    })
                    .collect(),
            },
        )),
    }
    .encode_to_vec())
}
fn attachment_values(values: &[proto::AttachmentRef]) -> Result<Vec<Attachment>> {
    values
        .iter()
        .map(|v| {
            Ok(Attachment {
                id: v.id.clone(),
                display_name: v.display_name.clone(),
                media_type: v.media_type.clone(),
                byte_length: v.byte_length,
                sha256: v
                    .sha256
                    .as_slice()
                    .try_into()
                    .map_err(|_| Error::Integrity)?,
            })
        })
        .collect()
}
fn preflight(type_name: &str, bytes: &[u8]) -> Result<()> {
    static POOL: std::sync::OnceLock<prost_reflect::DescriptorPool> = std::sync::OnceLock::new();
    let pool = POOL.get_or_init(|| {
        prost_reflect::DescriptorPool::decode(
            include_bytes!(concat!(env!("OUT_DIR"), "/transaction.descriptor.bin")).as_slice(),
        )
        .expect("compiled transaction descriptor")
    });
    crate::content::preflight(
        bytes,
        &pool
            .get_message_by_name(type_name)
            .expect("compiled message"),
        &mut 8192,
        0,
    )
}
// Schema 2 actions must be singular on the wire. Prost otherwise lets the last
// oneof field win, which could hide an old SetContent beside a versioned action.
fn preflight_versioned_action(mut raw: &[u8]) -> Result<()> {
    use prost::encoding::{DecodeContext, WireType, decode_key, skip_field};
    let mut action = None;
    while !raw.is_empty() {
        let (tag, wire) = decode_key(&mut raw).map_err(|_| Error::Invalid("stored command"))?;
        if (3..=8).contains(&tag) {
            if wire != WireType::LengthDelimited || action.replace(tag).is_some() {
                return Err(Error::Invalid("versioned action"));
            }
        }
        skip_field(wire, tag, &mut raw, DecodeContext::default())
            .map_err(|_| Error::Invalid("stored command"))?;
    }
    if !matches!(action, Some(7 | 8)) {
        return Err(Error::Invalid("versioned action"));
    }
    Ok(())
}
pub fn decode_command(raw: &[u8]) -> Result<proto::Command> {
    if raw.len() > MAX_EVENT_BYTES {
        return Err(Error::Limit);
    }
    preflight("morrow.transaction.v1.LegacyCommand", raw)?;
    let legacy = proto::LegacyCommand::decode(raw).map_err(|_| Error::Invalid("stored command"))?;
    let value = match legacy.schema_version {
        1 => proto::Command {
            schema_version: 1,
            operation_id: legacy.operation_id,
            action: legacy.action.map(|action| match action {
                proto::legacy_command::Action::CreateCard(v) => {
                    proto::command::Action::CreateCard(v)
                }
                proto::legacy_command::Action::Rename(v) => proto::command::Action::Rename(v),
                proto::legacy_command::Action::SetAttachments(v) => {
                    proto::command::Action::SetAttachments(v)
                }
                proto::legacy_command::Action::SetContent(v) => {
                    proto::command::Action::SetContent(v)
                }
            }),
        },
        2 => {
            if legacy.action.is_some() {
                return Err(Error::UnsupportedVersion);
            }
            preflight("morrow.transaction.v1.Command", raw)?;
            preflight_versioned_action(raw)?;
            proto::Command::decode(raw).map_err(|_| Error::Invalid("stored command"))?
        }
        _ => return Err(Error::UnsupportedVersion),
    };
    let versioned = matches!(
        &value.action,
        Some(
            proto::command::Action::MigrateContent(_)
                | proto::command::Action::SetVersionedContent(_)
        )
    );
    if value.schema_version != if versioned { 2 } else { 1 } {
        return Err(Error::UnsupportedVersion);
    }
    identity(&value.operation_id)?;
    match &value.action {
        Some(proto::command::Action::MigrateContent(change)) => {
            migration_value(&value.operation_id, change).validate()?;
        }
        Some(proto::command::Action::SetVersionedContent(change)) => {
            versioned_content_value(&value.operation_id, change)?.validate()?;
        }
        Some(proto::command::Action::CreateCard(bytes)) => {
            CardRecord::decode(bytes)?;
        }
        Some(proto::command::Action::Rename(rename)) => {
            RenameRequest {
                operation_id: value.operation_id.clone(),
                card_id: rename.card_id.clone(),
                expected_revision: rename.expected_revision,
                title: rename.title.clone(),
            }
            .validate()?;
        }
        Some(proto::command::Action::SetAttachments(change)) => {
            set_attachments_command(
                &value.operation_id,
                &change.card_id,
                change.expected_revision,
                &attachment_values(&change.attachments)?,
            )?;
        }
        Some(proto::command::Action::SetContent(change)) => {
            content_value(&value.operation_id, change)?.validate()?
        }
        None => return Err(Error::Invalid("stored action")),
    }
    Ok(value)
}
pub fn encode_commit(command: Vec<u8>, card: &CardRecord) -> Result<Vec<u8>> {
    encode_commit_with_evidence(command, card, &[])
}
/// Ordered evidence digests are host-supplied historical associations, not restored grants.
pub fn encode_commit_with_evidence(
    command: Vec<u8>,
    card: &CardRecord,
    evidence: &[[u8; 32]],
) -> Result<Vec<u8>> {
    if evidence.len() > MAX_TASK_EVIDENCE {
        return Err(Error::Limit);
    }
    let value = decode_command(&command)?;
    let summary = card.summary();
    if let Some(proto::command::Action::Rename(rename)) = &value.action
        && summary.title != rename.title
    {
        return Err(Error::Integrity);
    }
    if let Some(proto::command::Action::SetAttachments(change)) = &value.action
        && card.attachments() != attachment_values(&change.attachments)?
    {
        return Err(Error::Integrity);
    }
    if let Some(proto::command::Action::SetContent(change)) = &value.action
        && let Some(items) = &change.attachments
        && card.attachments() != attachment_values(&items.items)?
    {
        return Err(Error::Integrity);
    }
    if let Some(proto::command::Action::SetContent(change)) = &value.action
        && (summary.title != change.title
            || card.body() != change.body
            || summary.preview_text != change.preview_text)
    {
        return Err(Error::Integrity);
    }
    let commit = proto::Commit {
        schema_version: if evidence.is_empty() { 1 } else { 2 },
        task_evidence_sha256: evidence.iter().map(|d| d.to_vec()).collect(),
        attachment_sha256: card
            .attachments()
            .iter()
            .map(|v| v.sha256.to_vec())
            .collect(),
        command_sha256: Sha256::digest(&command).to_vec(),
        command,
        event_id: value.operation_id.clone(),
        operation_id: value.operation_id,
        card_id: summary.id,
        revision: summary.revision,
        content_sha256: Sha256::digest(card.encode()).to_vec(),
    };
    let bytes = envelope::pack(MAGIC, &commit.encode_to_vec(), MAX_EVENT_BYTES)?;
    decode_commit(&bytes)?;
    Ok(bytes)
}
pub fn decode_commit(bytes: &[u8]) -> Result<(proto::Commit, Receipt)> {
    let raw = envelope::unpack(MAGIC, bytes, MAX_EVENT_BYTES)?;
    preflight("morrow.transaction.v1.Commit", &raw)?;
    let commit =
        proto::Commit::decode(raw.as_slice()).map_err(|_| Error::Invalid("stored commit"))?;
    if !matches!(commit.schema_version, 1 | 2) {
        return Err(Error::UnsupportedVersion);
    }
    if (commit.schema_version == 1) != commit.task_evidence_sha256.is_empty()
        || commit.task_evidence_sha256.len() > MAX_TASK_EVIDENCE
        || commit.task_evidence_sha256.iter().any(|v| v.len() != 32)
    {
        return Err(Error::Integrity);
    }
    let command = decode_command(&commit.command)?;
    if commit.command_sha256 != Sha256::digest(&commit.command).as_slice()
        || command.operation_id != commit.operation_id
        || commit.event_id != commit.operation_id
        || commit.content_sha256.len() != 32
    {
        return Err(Error::Integrity);
    }
    identity(&commit.card_id)?;
    if commit.attachment_sha256.len() > 1024
        || commit.attachment_sha256.iter().any(|v| v.len() != 32)
    {
        return Err(Error::Integrity);
    }
    match command.action.unwrap() {
        proto::command::Action::SetVersionedContent(change) => {
            let edit = versioned_content_value(&commit.operation_id, &change)?;
            let result = edit.propose(&edit.source()?)?;
            if result.summary().id != commit.card_id
                || result.summary().revision != commit.revision
                || Sha256::digest(result.encode()).as_slice() != commit.content_sha256
                || result
                    .attachments()
                    .iter()
                    .map(|v| v.sha256.to_vec())
                    .collect::<Vec<_>>()
                    != commit.attachment_sha256
            {
                return Err(Error::Integrity);
            }
        }
        proto::command::Action::MigrateContent(change) => {
            let migration = migration_value(&commit.operation_id, &change);
            let source = migration.source()?;
            let result = migration.propose(&source)?;
            if result.summary().id != commit.card_id
                || result.summary().revision != commit.revision
                || Sha256::digest(result.encode()).as_slice() != commit.content_sha256
                || result
                    .attachments()
                    .iter()
                    .map(|v| v.sha256.to_vec())
                    .collect::<Vec<_>>()
                    != commit.attachment_sha256
            {
                return Err(Error::Integrity);
            }
        }
        proto::command::Action::SetContent(change) => {
            if change.card_id != commit.card_id
                || change.expected_revision.checked_add(1) != Some(commit.revision)
            {
                return Err(Error::Integrity);
            }
        }
        proto::command::Action::CreateCard(raw) => {
            let card = CardRecord::decode(&raw)?;
            if card
                .attachments()
                .iter()
                .map(|v| v.sha256.to_vec())
                .collect::<Vec<_>>()
                != commit.attachment_sha256
                || card.summary().id != commit.card_id
                || card.summary().revision != commit.revision
                || commit.content_sha256 != Sha256::digest(&raw).as_slice()
            {
                return Err(Error::Integrity);
            }
        }
        proto::command::Action::Rename(rename) => {
            if rename.card_id != commit.card_id
                || rename.expected_revision.checked_add(1) != Some(commit.revision)
            {
                return Err(Error::Integrity);
            }
        }
        proto::command::Action::SetAttachments(change) => {
            if change.card_id != commit.card_id
                || change.expected_revision.checked_add(1) != Some(commit.revision)
                || change
                    .attachments
                    .iter()
                    .map(|v| v.sha256.clone())
                    .collect::<Vec<_>>()
                    != commit.attachment_sha256
            {
                return Err(Error::Integrity);
            }
        }
    }
    let receipt = Receipt {
        operation_id: commit.operation_id.clone(),
        card_id: commit.card_id.clone(),
        revision: commit.revision,
        content_sha256: commit.content_sha256.as_slice().try_into().unwrap(),
        event_id: commit.event_id.clone(),
    };
    Ok((commit, receipt))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn legacy_unknown_field_cannot_acquire_new_migration_semantics() {
        let card = CardRecord::new("card", "unknown", 1, "original", vec![]).unwrap();
        let original = create_command("old", &card).unwrap();
        let mut historical = original.clone();
        // Field 7 was opaque to the v1 reader. Its contents are not a protobuf message.
        historical.extend_from_slice(&[0x3a, 3, 0xff, 0xff, 0xff]);
        assert_eq!(
            decode_command(&historical).unwrap(),
            decode_command(&original).unwrap()
        );
        decode_commit(&encode_commit(historical, &card).unwrap()).unwrap();
    }
    fn sample() -> Vec<u8> {
        let card = CardRecord::new("card", "unknown", 1, "old", vec![255]).unwrap();
        encode_commit(create_command("op", &card).unwrap(), &card).unwrap()
    }
    #[test]
    fn repeated_attachments_are_bounded_before_protobuf_allocation() {
        let command = proto::Command {
            schema_version: 1,
            operation_id: "op".into(),
            action: Some(proto::command::Action::SetAttachments(
                proto::SetAttachments {
                    card_id: "card".into(),
                    expected_revision: 1,
                    attachments: vec![proto::AttachmentRef::default(); 1025],
                },
            )),
        }
        .encode_to_vec();
        assert!(matches!(decode_command(&command), Err(Error::Limit)));
        let (mut record, _) = decode_commit(&sample()).unwrap();
        record.attachment_sha256 = vec![vec![0; 32]; 1025];
        let bytes = envelope::pack(MAGIC, &record.encode_to_vec(), MAX_EVENT_BYTES).unwrap();
        assert!(matches!(decode_commit(&bytes), Err(Error::Limit)));
    }
    #[test]
    fn transaction_framing_and_decompression_limits_reject_damaged_input() {
        let valid = sample();
        for length in 0..valid.len() {
            assert!(decode_commit(&valid[..length]).is_err());
        }
        let mut extra = valid.clone();
        extra.push(0);
        assert!(decode_commit(&extra).is_err());
        let mut corrupt = valid.clone();
        corrupt[18] ^= 1;
        assert!(matches!(decode_commit(&corrupt), Err(Error::Integrity)));
        let mut oversized = valid;
        oversized[10..14].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(matches!(decode_commit(&oversized), Err(Error::Limit)));
    }
    #[test]
    fn record_hash_does_not_substitute_for_command_and_result_correspondence() {
        let (record, _) = decode_commit(&sample()).unwrap();
        for change in 0..5 {
            let mut changed = record.clone();
            match change {
                0 => changed.command_sha256[0] ^= 1,
                1 => changed.operation_id = "other".into(),
                2 => changed.revision += 1,
                3 => changed.card_id = "other".into(),
                _ => changed.schema_version += 1,
            }
            let bytes = envelope::pack(MAGIC, &changed.encode_to_vec(), MAX_EVENT_BYTES).unwrap();
            assert!(decode_commit(&bytes).is_err());
        }
    }
    #[test]
    fn event_builder_rejects_wrong_card_or_wrong_revision() {
        let card = CardRecord::new("card", "unknown", 1, "old", vec![]).unwrap();
        let request = RenameRequest {
            operation_id: "op".into(),
            card_id: "card".into(),
            expected_revision: 1,
            title: "new".into(),
        };
        assert!(encode_commit(rename_command(&request).unwrap(), &card).is_err());
        let wrong_title = card.with_title(1, "wrong").unwrap();
        assert!(encode_commit(rename_command(&request).unwrap(), &wrong_title).is_err());
        let next = request.propose(&card).unwrap();
        assert_eq!(
            decode_commit(&encode_commit(rename_command(&request).unwrap(), &next).unwrap())
                .unwrap()
                .1
                .revision,
            2
        );
    }
}
