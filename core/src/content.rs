//! Own the complete record; expose read projections, never lossy save objects.
use crate::{Error, Result, identity, title};
use prost::Message;
use prost_reflect::{DescriptorPool, DynamicMessage, Value};
use std::sync::OnceLock;

pub const MAX_RECORD_BYTES: usize = 8 * 1024 * 1024;
pub const DESCRIPTOR: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/content.descriptor.bin"));
pub const CONTENT_SCHEMA: &[u8] = include_bytes!("../schemas/content.proto");
fn descriptor() -> prost_reflect::MessageDescriptor {
    static POOL: OnceLock<DescriptorPool> = OnceLock::new();
    POOL.get_or_init(|| DescriptorPool::decode(DESCRIPTOR).expect("compiled descriptor"))
        .get_message_by_name("morrow.content.v1.Card")
        .expect("compiled Card")
}
fn string(message: &DynamicMessage, name: &str) -> String {
    message
        .get_field_by_name(name)
        .unwrap()
        .as_str()
        .unwrap()
        .to_owned()
}
fn u32_field(message: &DynamicMessage, name: &str) -> u32 {
    message.get_field_by_name(name).unwrap().as_u32().unwrap()
}
fn revision(message: &DynamicMessage) -> u64 {
    message
        .get_field_by_name("revision")
        .unwrap()
        .as_u64()
        .unwrap()
}
// Bound allocations before reflection decoding. Opaque byte fields are not parsed.
fn preflight(
    mut bytes: &[u8],
    desc: &prost_reflect::MessageDescriptor,
    budget: &mut usize,
    depth: usize,
) -> Result<()> {
    use prost::encoding::{DecodeContext, WireType, decode_key, decode_varint, skip_field};
    if depth > 16 {
        return Err(Error::Limit);
    }
    let mut repeated = std::collections::HashMap::<u32, usize>::new();
    while !bytes.is_empty() {
        *budget = budget.checked_sub(1).ok_or(Error::Limit)?;
        let (number, wire) = decode_key(&mut bytes).map_err(|_| Error::Invalid("protobuf key"))?;
        let field = desc.get_field(number);
        if let Some(field) = &field
            && field.is_list()
        {
            let count = repeated.entry(number).or_default();
            *count += 1;
            if *count > 1024 {
                return Err(Error::Limit);
            }
        }
        if wire == WireType::LengthDelimited {
            let len = decode_varint(&mut bytes).map_err(|_| Error::Invalid("protobuf length"))?;
            if len > bytes.len() as u64 {
                return Err(Error::Invalid("protobuf length"));
            }
            let (payload, remaining) = bytes.split_at(len as usize);
            bytes = remaining;
            if let Some(field) = field
                && let prost_reflect::Kind::Message(nested) = field.kind()
            {
                preflight(payload, &nested, budget, depth + 1)?;
            }
        } else {
            // Deprecated proto2 groups are outside this proto3 contract.
            if matches!(wire, WireType::StartGroup | WireType::EndGroup) {
                return Err(Error::Invalid("protobuf group"));
            }
            skip_field(wire, number, &mut bytes, DecodeContext::default())
                .map_err(|_| Error::Invalid("protobuf field"))?;
        }
    }
    Ok(())
}
fn validate(message: &DynamicMessage) -> Result<()> {
    if u32_field(message, "schema_version") != 1 {
        return Err(Error::UnsupportedVersion);
    }
    identity(&string(message, "id"))?;
    identity(&string(message, "type_id"))?;
    if u32_field(message, "format_version") == 0 || revision(message) == 0 {
        return Err(Error::Invalid("format or revision"));
    }
    title(&string(message, "title"))?;
    for name in ["attachments", "relations"] {
        let field = message.get_field_by_name(name).unwrap();
        let list = field.as_list().unwrap();
        if list.len() > 1024 {
            return Err(Error::Limit);
        }
        for value in list {
            let item = value.as_message().unwrap();
            if name == "attachments" {
                identity(&string(item, "id"))?;
                let digest = item.get_field_by_name("sha256").unwrap();
                if digest.as_bytes().unwrap().len() != 32 {
                    return Err(Error::Invalid("blob digest"));
                }
            } else {
                identity(&string(item, "target_card_id"))?;
                identity(&string(item, "kind"))?;
            }
        }
    }
    if message.encoded_len() > MAX_RECORD_BYTES {
        return Err(Error::Limit);
    }
    Ok(())
}

#[derive(Clone, Debug)]
pub struct CardRecord {
    message: DynamicMessage,
    // Immutable decode source, kept apart from any subsequently edited encoding.
    original: Vec<u8>,
    edited: bool,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CardSummary {
    pub id: String,
    pub type_id: String,
    pub format_version: u32,
    pub revision: u64,
    pub title: String,
    pub preview_text: String,
}
impl CardRecord {
    pub fn new(
        id: &str,
        type_id: &str,
        format_version: u32,
        title_text: &str,
        body: Vec<u8>,
    ) -> Result<Self> {
        if body.len() > MAX_RECORD_BYTES {
            return Err(Error::Limit);
        }
        let mut message = DynamicMessage::new(descriptor());
        for (key, value) in [
            ("schema_version", Value::U32(1)),
            ("id", Value::String(id.into())),
            ("type_id", Value::String(type_id.into())),
            ("format_version", Value::U32(format_version)),
            ("revision", Value::U64(1)),
            ("title", Value::String(title_text.into())),
            ("body", Value::Bytes(body.into())),
        ] {
            message.set_field_by_name(key, value);
        }
        validate(&message)?;
        let original = message.encode_to_vec();
        Ok(Self {
            message,
            original,
            edited: false,
        })
    }
    pub fn decode(bytes: &[u8]) -> Result<Self> {
        if bytes.len() > MAX_RECORD_BYTES {
            return Err(Error::Limit);
        }
        preflight(bytes, &descriptor(), &mut 8192, 0)?;
        let message =
            DynamicMessage::decode(descriptor(), bytes).map_err(|_| Error::Invalid("protobuf"))?;
        validate(&message)?;
        Ok(Self {
            message,
            original: bytes.to_vec(),
            edited: false,
        })
    }
    pub fn original_bytes(&self) -> &[u8] {
        &self.original
    }
    pub fn encode(&self) -> Vec<u8> {
        if self.edited {
            self.message.encode_to_vec()
        } else {
            self.original.clone()
        }
    }
    pub fn summary(&self) -> CardSummary {
        let preview = self.message.get_field_by_name("preview").unwrap();
        CardSummary {
            id: string(&self.message, "id"),
            type_id: string(&self.message, "type_id"),
            format_version: u32_field(&self.message, "format_version"),
            revision: revision(&self.message),
            title: string(&self.message, "title"),
            preview_text: string(preview.as_message().unwrap(), "plain_text"),
        }
    }
    pub fn body(&self) -> Vec<u8> {
        self.message
            .get_field_by_name("body")
            .unwrap()
            .as_bytes()
            .unwrap()
            .to_vec()
    }
    /// Pure edit proposal. This does not persist, authorize, audit, or deduplicate.
    pub fn with_title(&self, expected_revision: u64, new_title: &str) -> Result<Self> {
        if expected_revision != revision(&self.message) {
            return Err(Error::RevisionConflict);
        }
        title(new_title)?;
        let next = expected_revision
            .checked_add(1)
            .ok_or(Error::Invalid("revision overflow"))?;
        let mut edited = self.clone();
        edited
            .message
            .set_field_by_name("title", Value::String(new_title.into()));
        edited
            .message
            .set_field_by_name("revision", Value::U64(next));
        validate(&edited.message)?;
        edited.edited = true;
        Ok(edited)
    }
}
