//! Portable persistent transaction contract, shared by native and future Web storage.
use crate::{Error, Result, content::CardRecord, envelope, identity, runtime::RenameRequest};
use prost::Message;
use sha2::{Digest, Sha256};
pub mod proto {
    include!(concat!(env!("OUT_DIR"), "/morrow.transaction.v1.rs"));
}
const MAGIC: &[u8; 8] = b"MORROWT1";
pub const MAX_EVENT_BYTES: usize = 16 * 1024 * 1024;
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
pub fn decode_command(raw: &[u8]) -> Result<proto::Command> {
    if raw.len() > MAX_EVENT_BYTES {
        return Err(Error::Limit);
    }
    let value = proto::Command::decode(raw).map_err(|_| Error::Invalid("stored command"))?;
    if value.schema_version != 1 {
        return Err(Error::UnsupportedVersion);
    }
    identity(&value.operation_id)?;
    match &value.action {
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
        None => return Err(Error::Invalid("stored action")),
    }
    Ok(value)
}
pub fn encode_commit(command: Vec<u8>, card: &CardRecord) -> Result<Vec<u8>> {
    let value = decode_command(&command)?;
    let summary = card.summary();
    if let Some(proto::command::Action::Rename(rename)) = &value.action
        && summary.title != rename.title
    {
        return Err(Error::Integrity);
    }
    let commit = proto::Commit {
        schema_version: 1,
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
    let commit =
        proto::Commit::decode(raw.as_slice()).map_err(|_| Error::Invalid("stored commit"))?;
    if commit.schema_version != 1 {
        return Err(Error::UnsupportedVersion);
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
    match command.action.unwrap() {
        proto::command::Action::CreateCard(raw) => {
            let card = CardRecord::decode(&raw)?;
            if card.summary().id != commit.card_id
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
    fn sample() -> Vec<u8> {
        let card = CardRecord::new("card", "unknown", 1, "old", vec![255]).unwrap();
        encode_commit(create_command("op", &card).unwrap(), &card).unwrap()
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
