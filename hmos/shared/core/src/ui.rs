//! Bounded UI descriptions and host-owned view event admission. No content writes.
use crate::{Error, Result, identity, ui_capnp as wire};
use capnp::{
    message::{Builder, ReaderOptions},
    serialize,
};
use std::collections::BTreeMap;
pub use wire::{EventKind, Kind, Tone};
pub const VERSION: u16 = 1;
pub const MAX_BYTES: usize = 65536;
pub const MAX_NODES: usize = 128;
pub const MAX_DOCUMENT_TEXT_BYTES: usize = 32768;
pub const MAX_DEPTH: usize = 8;
pub const MAX_TEXT_BYTES: usize = 4096;
pub fn schema_digest() -> [u8; 32] {
    crate::runtime::schema_digest(include_bytes!("../schemas/ui.capnp"))
}
fn invalid<T>(_: T) -> Error {
    Error::Invalid("ui message")
}
fn text(s: &str, max: usize) -> Result<()> {
    if s.len() > max {
        return Err(Error::Limit);
    }
    if s.chars().any(|c| c.is_control() && c != '\n' && c != '\t') {
        return Err(invalid(()));
    }
    Ok(())
}
fn reader(bytes: &[u8]) -> Result<capnp::message::Reader<serialize::BufferSegments<&[u8]>>> {
    if bytes.len() > MAX_BYTES {
        return Err(Error::Limit);
    }
    let mut rest = bytes;
    let r = serialize::read_message_from_flat_slice(
        &mut rest,
        ReaderOptions {
            traversal_limit_in_words: Some(MAX_BYTES / 8),
            nesting_limit: 16,
        },
    )
    .map_err(invalid)?;
    if !rest.is_empty() {
        return Err(invalid(()));
    }
    Ok(r)
}
fn bounded(m: &Builder<capnp::message::HeapAllocator>) -> Result<Vec<u8>> {
    let bytes = serialize::write_message_to_words(m);
    if bytes.len() > MAX_BYTES {
        Err(Error::Limit)
    } else {
        Ok(bytes)
    }
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Node {
    pub id: String,
    pub parent: String,
    pub kind: Kind,
    pub label: String,
    pub text: String,
    pub action: String,
    pub enabled: bool,
    pub checked: bool,
    pub max_bytes: u32,
    pub tone: Tone,
}
impl Node {
    pub fn new(id: &str, parent: &str, kind: Kind) -> Self {
        Self {
            id: id.into(),
            parent: parent.into(),
            kind,
            label: String::new(),
            text: String::new(),
            action: String::new(),
            enabled: true,
            checked: false,
            max_bytes: 0,
            tone: Tone::Normal,
        }
    }
    fn validate(&self) -> Result<()> {
        identity(&self.id)?;
        if !self.parent.is_empty() {
            identity(&self.parent)?;
        }
        text(&self.label, 512)?;
        text(&self.text, MAX_TEXT_BYTES)?;
        let interactive = matches!(self.kind, Kind::Button | Kind::TextInput | Kind::Toggle);
        if interactive {
            identity(&self.action)?;
            if self.label.is_empty() {
                return Err(invalid(()));
            }
        } else if !self.action.is_empty() || !self.label.is_empty() || !self.enabled {
            return Err(invalid(()));
        }
        if self.kind != Kind::Text && self.tone != Tone::Normal {
            return Err(invalid(()));
        }
        if self.kind != Kind::Toggle && self.checked {
            return Err(invalid(()));
        }
        if self.kind == Kind::TextInput {
            if self.max_bytes == 0
                || self.max_bytes as usize > MAX_TEXT_BYTES
                || self.text.len() > self.max_bytes as usize
            {
                return Err(Error::Limit);
            }
        } else if self.max_bytes != 0 {
            return Err(invalid(()));
        }
        if !matches!(self.kind, Kind::Text | Kind::TextInput) && !self.text.is_empty() {
            return Err(invalid(()));
        }
        Ok(())
    }
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Document {
    nodes: Vec<Node>,
}
impl Document {
    pub fn new(nodes: Vec<Node>) -> Result<Self> {
        if nodes.is_empty() || nodes.len() > MAX_NODES {
            return Err(Error::Limit);
        }
        let mut total_text_bytes = 0;
        let mut seen = BTreeMap::<&str, (usize, Kind)>::new();
        for (index, node) in nodes.iter().enumerate() {
            node.validate()?;
            total_text_bytes += node.id.len()
                + node.parent.len()
                + node.label.len()
                + node.text.len()
                + node.action.len();
            if total_text_bytes > MAX_DOCUMENT_TEXT_BYTES {
                return Err(Error::Limit);
            }
            let depth = if index == 0 {
                if !node.parent.is_empty() || node.kind != Kind::Column {
                    return Err(invalid(()));
                }
                1
            } else {
                let (d, k) = seen.get(node.parent.as_str()).ok_or(invalid(()))?;
                if !matches!(k, Kind::Column | Kind::Row) {
                    return Err(invalid(()));
                }
                d + 1
            };
            if depth > MAX_DEPTH {
                return Err(Error::Limit);
            }
            if seen.insert(&node.id, (depth, node.kind)).is_some() {
                return Err(invalid(()));
            }
        }
        let result = Self { nodes };
        result.encode()?;
        Ok(result)
    }
    pub fn nodes(&self) -> &[Node] {
        &self.nodes
    }
    pub fn encode(&self) -> Result<Vec<u8>> {
        let mut m = Builder::new_default();
        let mut r = m.init_root::<wire::document::Builder>();
        r.set_version(VERSION);
        r.set_schema_digest(&schema_digest());
        let mut nodes = r.init_nodes(self.nodes.len() as u32);
        for (i, n) in self.nodes.iter().enumerate() {
            let mut out = nodes.reborrow().get(i as u32);
            out.set_id(n.id.as_str());
            out.set_parent(n.parent.as_str());
            out.set_kind(n.kind);
            out.set_label(n.label.as_str());
            out.set_text(n.text.as_str());
            out.set_action(n.action.as_str());
            out.set_enabled(n.enabled);
            out.set_checked(n.checked);
            out.set_max_bytes(n.max_bytes);
            out.set_tone(n.tone);
        }
        bounded(&m)
    }
    pub fn decode(bytes: &[u8]) -> Result<Self> {
        let m = reader(bytes)?;
        let r = m.get_root::<wire::document::Reader>().map_err(invalid)?;
        if r.get_version() != VERSION || r.get_schema_digest().map_err(invalid)? != schema_digest()
        {
            return Err(Error::UnsupportedVersion);
        }
        let list = r.get_nodes().map_err(invalid)?;
        if list.len() as usize > MAX_NODES {
            return Err(Error::Limit);
        }
        let mut nodes = Vec::new();
        for n in list {
            nodes.push(Node {
                id: n
                    .get_id()
                    .map_err(invalid)?
                    .to_str()
                    .map_err(invalid)?
                    .into(),
                parent: n
                    .get_parent()
                    .map_err(invalid)?
                    .to_str()
                    .map_err(invalid)?
                    .into(),
                kind: n.get_kind().map_err(invalid)?,
                label: n
                    .get_label()
                    .map_err(invalid)?
                    .to_str()
                    .map_err(invalid)?
                    .into(),
                text: n
                    .get_text()
                    .map_err(invalid)?
                    .to_str()
                    .map_err(invalid)?
                    .into(),
                action: n
                    .get_action()
                    .map_err(invalid)?
                    .to_str()
                    .map_err(invalid)?
                    .into(),
                enabled: n.get_enabled(),
                checked: n.get_checked(),
                max_bytes: n.get_max_bytes(),
                tone: n.get_tone().map_err(invalid)?,
            });
        }
        Self::new(nodes)
    }
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Event {
    pub view: String,
    pub generation: u64,
    pub revision: u64,
    pub serial: u64,
    pub node: String,
    pub action: String,
    pub kind: EventKind,
    pub text: String,
    pub checked: bool,
}
impl Event {
    fn validate(&self) -> Result<()> {
        identity(&self.view)?;
        identity(&self.node)?;
        identity(&self.action)?;
        text(&self.text, MAX_TEXT_BYTES)?;
        if self.generation == 0 || self.revision == 0 || self.serial == 0 {
            return Err(invalid(()));
        }
        if self.kind != EventKind::EditText && !self.text.is_empty() {
            return Err(invalid(()));
        }
        if self.kind != EventKind::SetToggle && self.checked {
            return Err(invalid(()));
        }
        Ok(())
    }
    pub fn encode(&self) -> Result<Vec<u8>> {
        self.validate()?;
        let mut m = Builder::new_default();
        let mut r = m.init_root::<wire::event::Builder>();
        r.set_version(VERSION);
        r.set_schema_digest(&schema_digest());
        r.set_view(self.view.as_str());
        r.set_generation(self.generation);
        r.set_revision(self.revision);
        r.set_serial(self.serial);
        r.set_node(self.node.as_str());
        r.set_action(self.action.as_str());
        r.set_kind(self.kind);
        r.set_text(self.text.as_str());
        r.set_checked(self.checked);
        bounded(&m)
    }
    pub fn decode(bytes: &[u8]) -> Result<Self> {
        let m = reader(bytes)?;
        let r = m.get_root::<wire::event::Reader>().map_err(invalid)?;
        if r.get_version() != VERSION || r.get_schema_digest().map_err(invalid)? != schema_digest()
        {
            return Err(Error::UnsupportedVersion);
        }
        let result = Self {
            view: r
                .get_view()
                .map_err(invalid)?
                .to_str()
                .map_err(invalid)?
                .into(),
            generation: r.get_generation(),
            revision: r.get_revision(),
            serial: r.get_serial(),
            node: r
                .get_node()
                .map_err(invalid)?
                .to_str()
                .map_err(invalid)?
                .into(),
            action: r
                .get_action()
                .map_err(invalid)?
                .to_str()
                .map_err(invalid)?
                .into(),
            kind: r.get_kind().map_err(invalid)?,
            text: r
                .get_text()
                .map_err(invalid)?
                .to_str()
                .map_err(invalid)?
                .into(),
            checked: r.get_checked(),
        };
        result.validate()?;
        Ok(result)
    }
}
/// Host chooses view/generation and binds this session to the actual plugin connection.
/// Accepted events are routing data; content permissions are still independently required.
pub struct Session {
    view: String,
    generation: u64,
    revision: u64,
    serial: u64,
    document: Option<Document>,
    closed: bool,
}
impl Session {
    pub fn new(view: &str, generation: u64) -> Result<Self> {
        identity(view)?;
        if generation == 0 {
            return Err(invalid(()));
        }
        Ok(Self {
            view: view.into(),
            generation,
            revision: 0,
            serial: 0,
            document: None,
            closed: false,
        })
    }
    pub fn replace(&mut self, base_revision: u64, document: Document) -> Result<u64> {
        if self.closed {
            return Err(invalid(()));
        }
        if base_revision != self.revision {
            return Err(Error::RevisionConflict);
        }
        let revision = self.revision.checked_add(1).ok_or(Error::Limit)?;
        self.document = Some(document);
        self.revision = revision;
        Ok(revision)
    }
    pub fn accept(&mut self, bytes: &[u8]) -> Result<Event> {
        if self.closed {
            return Err(invalid(()));
        }
        let e = Event::decode(bytes)?;
        if e.view != self.view
            || e.generation != self.generation
            || e.revision != self.revision
            || Some(e.serial) != self.serial.checked_add(1)
        {
            return Err(invalid(()));
        }
        let n = self
            .document
            .as_ref()
            .and_then(|d| d.nodes.iter().find(|n| n.id == e.node))
            .ok_or(invalid(()))?;
        if !n.enabled || n.action != e.action {
            return Err(invalid(()));
        }
        match (n.kind, e.kind) {
            (Kind::Button, EventKind::Activate) | (Kind::Toggle, EventKind::SetToggle) => {}
            (Kind::TextInput, EventKind::EditText) if e.text.len() <= n.max_bytes as usize => {}
            _ => return Err(invalid(())),
        }
        self.serial = e.serial;
        Ok(e)
    }
    pub fn close(&mut self) {
        self.closed = true;
        self.document = None;
    }
    pub fn revision(&self) -> u64 {
        self.revision
    }
}
