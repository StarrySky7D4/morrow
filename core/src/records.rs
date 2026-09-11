//! Portable workspace, placement and draft records. Mutations preserve unknown fields.
use crate::{Error, Result, content, envelope, identity, title};
use prost::Message;
use prost_reflect::{DescriptorPool, DynamicMessage, Value};
use sha2::{Digest, Sha256};
use std::sync::OnceLock;
pub mod proto {
    include!(concat!(env!("OUT_DIR"), "/morrow.record_transaction.v1.rs"));
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u32)]
pub enum Kind {
    Workspace = 1,
    Placement = 2,
    Draft = 3,
}
impl TryFrom<u32> for Kind {
    type Error = Error;
    fn try_from(v: u32) -> Result<Self> {
        match v {
            1 => Ok(Self::Workspace),
            2 => Ok(Self::Placement),
            3 => Ok(Self::Draft),
            _ => Err(Error::Invalid("record kind")),
        }
    }
}
impl Kind {
    fn name(self) -> &'static str {
        match self {
            Self::Workspace => "Workspace",
            Self::Placement => "ViewPlacement",
            Self::Draft => "Draft",
        }
    }
    fn magic(self) -> &'static [u8; 8] {
        match self {
            Self::Workspace => b"MORROWW1",
            Self::Placement => b"MORROWV1",
            Self::Draft => b"MORROWD1",
        }
    }
    fn descriptor(self) -> prost_reflect::MessageDescriptor {
        static POOL: OnceLock<DescriptorPool> = OnceLock::new();
        POOL.get_or_init(|| DescriptorPool::decode(content::DESCRIPTOR).expect("compiled content"))
            .get_message_by_name(&format!("morrow.content.v1.{}", self.name()))
            .expect("compiled record")
    }
}
#[derive(Clone, Debug)]
pub struct Record {
    kind: Kind,
    message: DynamicMessage,
    raw: Vec<u8>,
}
impl Record {
    fn new(kind: Kind, id: &str, fields: Vec<(&str, Value)>) -> Result<Self> {
        let mut message = DynamicMessage::new(kind.descriptor());
        for (k, v) in [
            ("schema_version", Value::U32(1)),
            ("id", Value::String(id.into())),
            ("revision", Value::U64(1)),
        ]
        .into_iter()
        .chain(fields)
        {
            message.set_field_by_name(k, v);
        }
        Self::decode(kind, &message.encode_to_vec())
    }
    pub fn workspace(id: &str, title: &str) -> Result<Self> {
        Self::new(
            Kind::Workspace,
            id,
            vec![("title", Value::String(title.into()))],
        )
    }
    pub fn placement(id: &str, workspace: &str, card: &str, order: i64) -> Result<Self> {
        Self::new(
            Kind::Placement,
            id,
            vec![
                ("workspace_id", Value::String(workspace.into())),
                ("card_id", Value::String(card.into())),
                ("order_key", Value::I64(order)),
                ("width_units", Value::U32(1)),
            ],
        )
    }
    pub fn draft(
        id: &str,
        card: &str,
        base: u64,
        type_id: &str,
        format: u32,
        body: Vec<u8>,
    ) -> Result<Self> {
        if body.len() > content::MAX_RECORD_BYTES {
            return Err(Error::Limit);
        }
        Self::new(
            Kind::Draft,
            id,
            vec![
                ("card_id", Value::String(card.into())),
                ("base_revision", Value::U64(base)),
                ("type_id", Value::String(type_id.into())),
                ("format_version", Value::U32(format)),
                ("body", Value::Bytes(body.into())),
            ],
        )
    }
    pub fn decode(kind: Kind, bytes: &[u8]) -> Result<Self> {
        if bytes.len() > content::MAX_RECORD_BYTES {
            return Err(Error::Limit);
        }
        let desc = kind.descriptor();
        content::preflight(bytes, &desc, &mut 8192, 0)?;
        let message =
            DynamicMessage::decode(desc, bytes).map_err(|_| Error::Invalid("record protobuf"))?;
        let value = Self {
            kind,
            message,
            raw: bytes.to_vec(),
        };
        value.validate()?;
        Ok(value)
    }
    fn validate(&self) -> Result<()> {
        if self.number32("schema_version") != 1 {
            return Err(Error::UnsupportedVersion);
        }
        identity(&self.id())?;
        if self.revision() == 0 {
            return Err(Error::Invalid("record revision"));
        }
        match self.kind {
            Kind::Workspace => title(&self.text("title"))?,
            Kind::Placement => {
                identity(&self.text("workspace_id"))?;
                identity(&self.text("card_id"))?;
                if !(1..=64).contains(&self.number32("width_units")) {
                    return Err(Error::Limit);
                }
            }
            Kind::Draft => {
                identity(&self.text("card_id"))?;
                identity(&self.text("type_id"))?;
                if self.number64("base_revision") == 0 || self.number32("format_version") == 0 {
                    return Err(Error::Invalid("draft base"));
                }
            }
        }
        Ok(())
    }
    pub fn kind(&self) -> Kind {
        self.kind
    }
    pub fn id(&self) -> String {
        self.text("id")
    }
    pub fn revision(&self) -> u64 {
        self.number64("revision")
    }
    pub fn text(&self, field: &str) -> String {
        self.message
            .get_field_by_name(field)
            .and_then(|v| v.as_str().map(str::to_owned))
            .unwrap_or_default()
    }
    pub fn number64(&self, field: &str) -> u64 {
        self.message
            .get_field_by_name(field)
            .and_then(|v| v.as_u64())
            .unwrap_or_default()
    }
    pub fn number32(&self, field: &str) -> u32 {
        self.message
            .get_field_by_name(field)
            .and_then(|v| v.as_u32())
            .unwrap_or_default()
    }
    pub fn encode(&self) -> Vec<u8> {
        self.raw.clone()
    }
    pub fn container(&self) -> Result<Vec<u8>> {
        envelope::pack(self.kind.magic(), &self.raw, content::MAX_RECORD_BYTES)
    }
    pub fn from_container(kind: Kind, bytes: &[u8]) -> Result<Self> {
        Self::decode(
            kind,
            &envelope::unpack(kind.magic(), bytes, content::MAX_RECORD_BYTES)?,
        )
    }
    pub fn patched(&self, expected: u64, patch: &proto::Patch) -> Result<Self> {
        validate_patch(self.kind, patch)?;
        if self.revision() != expected {
            return Err(Error::RevisionConflict);
        }
        let revision = expected.checked_add(1).ok_or(Error::Limit)?;
        let mut next = self.message.clone();
        next.set_field_by_name("revision", Value::U64(revision));
        for (key, val) in [
            ("title", patch.title.clone().map(Value::String)),
            ("body", patch.body.clone().map(|v| Value::Bytes(v.into()))),
            ("order_key", patch.order_key.map(Value::I64)),
            ("collapsed", patch.collapsed.map(Value::Bool)),
            ("width_units", patch.width_units.map(Value::U32)),
        ] {
            if let Some(v) = val {
                next.set_field_by_name(key, v);
            }
        }
        Self::decode(self.kind, &next.encode_to_vec())
    }
}
fn validate_patch(kind: Kind, p: &proto::Patch) -> Result<()> {
    let title_present = p.title.is_some();
    let body = p.body.is_some();
    let layout = p.order_key.is_some() || p.collapsed.is_some() || p.width_units.is_some();
    let valid = match kind {
        Kind::Workspace => title_present && !body && !layout,
        Kind::Draft => body && !title_present && !layout,
        Kind::Placement => layout && !title_present && !body,
    };
    if !valid {
        return Err(Error::Invalid("record patch scope"));
    }
    if let Some(v) = &p.title {
        title(v)?;
    }
    if p.body
        .as_ref()
        .is_some_and(|v| v.len() > content::MAX_RECORD_BYTES)
        || p.width_units.is_some_and(|v| !(1..=64).contains(&v))
    {
        return Err(Error::Limit);
    }
    Ok(())
}
fn preflight(name: &str, raw: &[u8]) -> Result<()> {
    if raw.len() > crate::transaction::MAX_EVENT_BYTES {
        return Err(Error::Limit);
    }
    static POOL: OnceLock<DescriptorPool> = OnceLock::new();
    let desc = POOL
        .get_or_init(|| {
            DescriptorPool::decode(
                include_bytes!(concat!(
                    env!("OUT_DIR"),
                    "/record_transaction.descriptor.bin"
                ))
                .as_slice(),
            )
            .expect("compiled record transaction")
        })
        .get_message_by_name(name)
        .expect("compiled message");
    content::preflight(raw, &desc, &mut 8192, 0)
}
pub fn create_command(operation: &str, record: &Record) -> Result<Vec<u8>> {
    let value = proto::Command {
        schema_version: 1,
        operation_id: operation.into(),
        kind: record.kind as u32,
        object_id: record.id(),
        expected_revision: 0,
        action: Some(proto::command::Action::Create(record.encode())),
    };
    let raw = value.encode_to_vec();
    decode_command(&raw)?;
    Ok(raw)
}
pub fn patch_command(
    operation: &str,
    kind: Kind,
    id: &str,
    expected: u64,
    patch: proto::Patch,
) -> Result<Vec<u8>> {
    let value = proto::Command {
        schema_version: 1,
        operation_id: operation.into(),
        kind: kind as u32,
        object_id: id.into(),
        expected_revision: expected,
        action: Some(proto::command::Action::Patch(patch)),
    };
    let raw = value.encode_to_vec();
    decode_command(&raw)?;
    Ok(raw)
}
pub fn decode_command(raw: &[u8]) -> Result<proto::Command> {
    preflight("morrow.record_transaction.v1.Command", raw)?;
    let v = proto::Command::decode(raw).map_err(|_| Error::Invalid("record command"))?;
    if v.schema_version != 1 {
        return Err(Error::UnsupportedVersion);
    }
    identity(&v.operation_id)?;
    identity(&v.object_id)?;
    let kind = Kind::try_from(v.kind)?;
    match &v.action {
        Some(proto::command::Action::Create(raw)) => {
            let record = Record::decode(kind, raw)?;
            if v.expected_revision != 0 || record.revision() != 1 || record.id() != v.object_id {
                return Err(Error::Invalid("record creation"));
            }
        }
        Some(proto::command::Action::Patch(p)) => {
            if v.expected_revision == 0 {
                return Err(Error::Invalid("record revision"));
            }
            validate_patch(kind, p)?;
        }
        None => return Err(Error::Invalid("record action")),
    };
    Ok(v)
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Receipt {
    pub operation_id: String,
    pub kind: Kind,
    pub object_id: String,
    pub revision: u64,
    pub content_sha256: [u8; 32],
}
pub fn encode_commit(command: Vec<u8>, record: &Record) -> Result<Vec<u8>> {
    let event = proto::Commit {
        schema_version: 1,
        command_sha256: Sha256::digest(&command).to_vec(),
        command,
        result: record.encode(),
        result_sha256: Sha256::digest(record.encode()).to_vec(),
    };
    let raw = envelope::pack(
        b"MORROWR1",
        &event.encode_to_vec(),
        crate::transaction::MAX_EVENT_BYTES,
    )?;
    decode_commit(&raw)?;
    Ok(raw)
}
pub fn decode_commit(raw: &[u8]) -> Result<(proto::Commit, Receipt)> {
    let bytes = envelope::unpack(b"MORROWR1", raw, crate::transaction::MAX_EVENT_BYTES)?;
    preflight("morrow.record_transaction.v1.Commit", &bytes)?;
    let event = proto::Commit::decode(bytes.as_slice()).map_err(|_| Error::Integrity)?;
    if event.schema_version != 1 {
        return Err(Error::UnsupportedVersion);
    }
    let cmd = decode_command(&event.command)?;
    let kind = Kind::try_from(cmd.kind)?;
    let record = Record::decode(kind, &event.result)?;
    if event.command_sha256 != Sha256::digest(&event.command).as_slice()
        || event.result_sha256 != Sha256::digest(&event.result).as_slice()
        || record.id() != cmd.object_id
        || cmd.expected_revision.checked_add(1) != Some(record.revision())
    {
        return Err(Error::Integrity);
    }
    match cmd.action.unwrap() {
        proto::command::Action::Create(raw) => {
            if raw != event.result {
                return Err(Error::Integrity);
            }
        }
        proto::command::Action::Patch(p) => {
            for (k, v) in [
                ("title", p.title.map(Value::String)),
                ("body", p.body.map(|v| Value::Bytes(v.into()))),
                ("order_key", p.order_key.map(Value::I64)),
                ("collapsed", p.collapsed.map(Value::Bool)),
                ("width_units", p.width_units.map(Value::U32)),
            ] {
                if let Some(v) = v
                    && record.message.get_field_by_name(k).as_deref() != Some(&v)
                {
                    return Err(Error::Integrity);
                }
            }
        }
    }
    let receipt = Receipt {
        operation_id: cmd.operation_id,
        kind,
        object_id: record.id(),
        revision: record.revision(),
        content_sha256: event
            .result_sha256
            .as_slice()
            .try_into()
            .map_err(|_| Error::Integrity)?,
    };
    Ok((event, receipt))
}
