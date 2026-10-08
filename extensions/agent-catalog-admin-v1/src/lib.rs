//! Independent private native catalog administration; no live authority is serialized.
use capnp::{
    message::{Builder, ReaderOptions},
    serialize,
};
use sha2::{Digest, Sha256};
pub mod agent_catalog_capnp {
    include!(concat!(env!("OUT_DIR"), "/agent_catalog_capnp.rs"));
}
include!(concat!(env!("OUT_DIR"), "/schema_digest.rs"));
use agent_catalog_capnp as wire;

pub const MAGIC: &[u8; 8] = b"MROWCA15";
pub const VERSION: u16 = 1;
pub const MAX_FRAME_BYTES: usize = 128 * 1024;
pub const MAX_PAGE: usize = 16;
pub const MAX_SCOPE: usize = 16;
pub const MAX_PATH_BYTES: usize = 4096;
pub const MAX_ID_BYTES: usize = 256;
pub const MAX_VERSION_BYTES: usize = 128;
pub const MAX_SCOPE_BYTES: usize = 128;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    Invalid,
    Limit,
    Contract,
    Correlation,
}
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for Error {}
pub type Result<T> = std::result::Result<T, Error>;
fn parsed<T>(r: capnp::Result<T>) -> Result<T> {
    r.map_err(|_| Error::Contract)
}
fn text(r: capnp::Result<capnp::text::Reader<'_>>) -> Result<String> {
    Ok(parsed(r)?.to_str().map_err(|_| Error::Contract)?.to_owned())
}
fn bytes<const N: usize>(r: capnp::Result<&[u8]>) -> Result<[u8; N]> {
    parsed(r)?.try_into().map_err(|_| Error::Contract)
}
pub fn hash(bytes: &[u8]) -> [u8; 32] {
    Sha256::digest(bytes).into()
}
pub fn schema_digest() -> [u8; 32] {
    SCHEMA_DIGEST
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Revisions {
    pub catalog: u64,
    pub manager: u64,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Approval {
    pub session_bits: u16,
    pub process_bits: u16,
    pub sessions: Vec<String>,
    pub domain: String,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Review {
    pub id: String,
    pub version: String,
    pub full_sha256: [u8; 32],
    pub base_sha256: [u8; 32],
    pub session_schema: [u8; 32],
    pub process_schema: [u8; 32],
    pub session_bits: u16,
    pub process_bits: u16,
    pub sessions: Vec<String>,
    pub domain: String,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    pub review: Review,
    pub selected: bool,
    pub enabled: bool,
    pub approval: Option<Approval>,
    pub base_selected: bool,
    pub base_enabled: bool,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Action {
    State,
    Inspect {
        path: String,
    },
    Install {
        path: String,
        full_sha256: [u8; 32],
        revisions: Revisions,
    },
    BaseSelect {
        full_sha256: [u8; 32],
        revisions: Revisions,
    },
    BaseEnable {
        id: String,
        full_sha256: [u8; 32],
        enabled: bool,
        revisions: Revisions,
    },
    WrapperSelect {
        full_sha256: [u8; 32],
        revisions: Revisions,
    },
    Approve {
        id: String,
        full_sha256: [u8; 32],
        approval: Approval,
        revisions: Revisions,
    },
    WrapperEnable {
        id: String,
        full_sha256: [u8; 32],
        enabled: bool,
        revisions: Revisions,
    },
    Remove {
        id: String,
        full_sha256: [u8; 32],
        revisions: Revisions,
    },
    Page {
        after: Option<String>,
        limit: u16,
        revisions: Revisions,
    },
}
impl Action {
    pub fn tag(&self) -> u16 {
        match self {
            Self::State => 0,
            Self::Inspect { .. } => 1,
            Self::Install { .. } => 2,
            Self::BaseSelect { .. } => 3,
            Self::BaseEnable { .. } => 4,
            Self::WrapperSelect { .. } => 5,
            Self::Approve { .. } => 6,
            Self::WrapperEnable { .. } => 7,
            Self::Remove { .. } => 8,
            Self::Page { .. } => 9,
        }
    }
    pub fn revisions(&self) -> Option<Revisions> {
        match self {
            Self::State | Self::Inspect { .. } => None,
            Self::Install { revisions, .. }
            | Self::BaseSelect { revisions, .. }
            | Self::BaseEnable { revisions, .. }
            | Self::WrapperSelect { revisions, .. }
            | Self::Approve { revisions, .. }
            | Self::WrapperEnable { revisions, .. }
            | Self::Remove { revisions, .. }
            | Self::Page { revisions, .. } => Some(*revisions),
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Request {
    pub id: [u8; 16],
    pub action: Action,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    Ok,
    Invalid,
    Conflict,
    Denied,
    NotFound,
    Limit,
    Storage,
    Unknown,
    Busy,
    RecoveryRequired,
    OwnerUnavailable,
    Unsupported,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Body {
    None,
    Review(Review),
    Page {
        entries: Vec<Entry>,
        next: Option<String>,
    },
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Outcome {
    pub status: Status,
    pub revisions: Revisions,
    pub body: Body,
}
impl Outcome {
    pub fn failure(status: Status, revisions: Revisions) -> Self {
        Self::error(status, revisions)
    }
    pub fn error(status: Status, revisions: Revisions) -> Self {
        Self {
            status,
            revisions,
            body: Body::None,
        }
    }
    pub fn ok(revisions: Revisions, body: Body) -> Self {
        Self {
            status: Status::Ok,
            revisions,
            body,
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Reply {
    pub id: [u8; 16],
    pub request_sha256: [u8; 32],
    pub action: u16,
    pub outcome: Outcome,
}
pub trait Target {
    fn execute(&mut self, request: &Request) -> Outcome;
}

fn bounded(v: &str, max: usize) -> bool {
    !v.is_empty() && v.len() <= max && !v.contains('\0')
}
fn scope(v: &str) -> bool {
    bounded(v, MAX_SCOPE_BYTES)
        && v.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"._:-".contains(&b))
}
fn scopes(v: &[String]) -> bool {
    !v.is_empty()
        && v.len() <= MAX_SCOPE
        && v.iter().all(|s| scope(s))
        && v.windows(2).all(|s| s[0] < s[1])
}
fn cursor(v: &str) -> bool {
    v.len() == 64
        && v.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
fn approval(v: &Approval) -> Result<()> {
    if v.session_bits & !31 != 0
        || v.session_bits & 1 == 0
        || v.process_bits & !127 != 0
        || !scopes(&v.sessions)
        || !scope(&v.domain)
    {
        return Err(Error::Invalid);
    }
    Ok(())
}
fn review(v: &Review) -> Result<()> {
    if !bounded(&v.id, MAX_ID_BYTES) || !bounded(&v.version, MAX_VERSION_BYTES) {
        return Err(Error::Invalid);
    }
    approval(&Approval {
        session_bits: v.session_bits,
        process_bits: v.process_bits,
        sessions: v.sessions.clone(),
        domain: v.domain.clone(),
    })
}
fn entry(v: &Entry) -> Result<()> {
    review(&v.review)?;
    if v.base_enabled && !v.base_selected
        || v.enabled && (!v.selected || v.approval.is_none())
        || v.approval.is_some() && !v.selected
    {
        return Err(Error::Contract);
    }
    if let Some(a) = &v.approval {
        approval(a)?;
        if a.session_bits & !v.review.session_bits != 0
            || a.process_bits & !v.review.process_bits != 0
            || a.domain != v.review.domain
            || a.sessions.iter().any(|s| !v.review.sessions.contains(s))
        {
            return Err(Error::Contract);
        }
    }
    Ok(())
}
fn set_revisions(mut w: wire::revisions::Builder<'_>, v: Revisions) {
    w.set_catalog(v.catalog);
    w.set_manager(v.manager);
}
fn get_revisions(w: wire::revisions::Reader<'_>) -> Revisions {
    Revisions {
        catalog: w.get_catalog(),
        manager: w.get_manager(),
    }
}
fn set_approval(mut w: wire::approval::Builder<'_>, v: &Approval) {
    w.set_session_bits(v.session_bits);
    w.set_process_bits(v.process_bits);
    {
        let mut s = w.reborrow().init_sessions(v.sessions.len() as u32);
        for (i, v) in v.sessions.iter().enumerate() {
            s.set(i as u32, v.as_str());
        }
    }
    w.set_domain(v.domain.as_str());
}
fn get_scopes(r: capnp::Result<capnp::text_list::Reader<'_>>) -> Result<Vec<String>> {
    let r = parsed(r)?;
    if r.len() as usize > MAX_SCOPE {
        return Err(Error::Limit);
    }
    r.iter().map(text).collect()
}
fn get_approval(w: wire::approval::Reader<'_>) -> Result<Approval> {
    let v = Approval {
        session_bits: w.get_session_bits(),
        process_bits: w.get_process_bits(),
        sessions: get_scopes(w.get_sessions())?,
        domain: text(w.get_domain())?,
    };
    approval(&v)?;
    Ok(v)
}
fn set_review(mut w: wire::review::Builder<'_>, v: &Review) {
    w.set_id(v.id.as_str());
    w.set_version(v.version.as_str());
    w.set_full_sha256(&v.full_sha256);
    w.set_base_sha256(&v.base_sha256);
    w.set_session_schema(&v.session_schema);
    w.set_process_schema(&v.process_schema);
    w.set_session_bits(v.session_bits);
    w.set_process_bits(v.process_bits);
    {
        let mut s = w.reborrow().init_sessions(v.sessions.len() as u32);
        for (i, v) in v.sessions.iter().enumerate() {
            s.set(i as u32, v.as_str());
        }
    }
    w.set_domain(v.domain.as_str());
}
fn get_review(w: wire::review::Reader<'_>) -> Result<Review> {
    let v = Review {
        id: text(w.get_id())?,
        version: text(w.get_version())?,
        full_sha256: bytes(w.get_full_sha256())?,
        base_sha256: bytes(w.get_base_sha256())?,
        session_schema: bytes(w.get_session_schema())?,
        process_schema: bytes(w.get_process_schema())?,
        session_bits: w.get_session_bits(),
        process_bits: w.get_process_bits(),
        sessions: get_scopes(w.get_sessions())?,
        domain: text(w.get_domain())?,
    };
    review(&v)?;
    Ok(v)
}
fn set_entry(mut w: wire::entry::Builder<'_>, v: &Entry) {
    set_review(w.reborrow().init_review(), &v.review);
    w.set_selected(v.selected);
    w.set_enabled(v.enabled);
    if let Some(a) = &v.approval {
        set_approval(w.reborrow().init_approval(), a);
    }
    w.set_base_selected(v.base_selected);
    w.set_base_enabled(v.base_enabled);
}
fn get_entry(w: wire::entry::Reader<'_>) -> Result<Entry> {
    let v = Entry {
        review: get_review(parsed(w.get_review())?)?,
        selected: w.get_selected(),
        enabled: w.get_enabled(),
        approval: if w.has_approval() {
            Some(get_approval(parsed(w.get_approval())?)?)
        } else {
            None
        },
        base_selected: w.get_base_selected(),
        base_enabled: w.get_base_enabled(),
    };
    entry(&v)?;
    Ok(v)
}
fn frame(builder: &Builder<capnp::message::HeapAllocator>) -> Result<Vec<u8>> {
    let mut v = MAGIC.to_vec();
    v.extend(serialize::write_message_to_words(builder));
    if v.len() > MAX_FRAME_BYTES {
        return Err(Error::Limit);
    }
    Ok(v)
}
fn builder() -> Builder<capnp::message::HeapAllocator> {
    Builder::new(
        capnp::message::HeapAllocator::new().first_segment_words((MAX_FRAME_BYTES / 8) as u32),
    )
}
fn reader(bytes: &[u8]) -> Result<capnp::message::Reader<serialize::BufferSegments<&[u8]>>> {
    if bytes.len() > MAX_FRAME_BYTES {
        return Err(Error::Limit);
    }
    if bytes.len() < 16 || &bytes[..8] != MAGIC {
        return Err(Error::Contract);
    }
    let mut rest = &bytes[8..];
    let r = parsed(serialize::read_message_from_flat_slice(
        &mut rest,
        ReaderOptions {
            traversal_limit_in_words: Some(16384),
            nesting_limit: 12,
        },
    ))?;
    if !rest.is_empty() {
        return Err(Error::Contract);
    }
    Ok(r)
}
impl Request {
    pub fn is_mutation(&self) -> bool {
        matches!(
            self.action,
            Action::Install { .. }
                | Action::BaseSelect { .. }
                | Action::BaseEnable { .. }
                | Action::WrapperSelect { .. }
                | Action::Approve { .. }
                | Action::WrapperEnable { .. }
                | Action::Remove { .. }
        )
    }
    pub fn validate(&self) -> Result<()> {
        if self.id == [0; 16] {
            return Err(Error::Invalid);
        }
        match &self.action {
            Action::Inspect { path } | Action::Install { path, .. }
                if !bounded(path, MAX_PATH_BYTES) =>
            {
                return Err(Error::Invalid);
            }
            Action::BaseEnable { id, .. }
            | Action::Approve { id, .. }
            | Action::WrapperEnable { id, .. }
            | Action::Remove { id, .. }
                if !bounded(id, MAX_ID_BYTES) =>
            {
                return Err(Error::Invalid);
            }
            Action::Page { after, limit, .. }
                if *limit == 0
                    || usize::from(*limit) > MAX_PAGE
                    || after.as_ref().is_some_and(|v| !cursor(v)) =>
            {
                return Err(Error::Limit);
            }
            _ => {}
        }
        if let Action::Approve { approval: a, .. } = &self.action {
            approval(a)?;
        }
        Ok(())
    }
    pub fn encode(&self) -> Result<Vec<u8>> {
        self.validate()?;
        let mut b = builder();
        {
            let mut w = b.init_root::<wire::request::Builder>();
            w.set_version(VERSION);
            w.set_action(wire::Action::try_from(self.action.tag()).map_err(|_| Error::Invalid)?);
            w.set_digest(&SCHEMA_DIGEST);
            w.set_id(&self.id);
            if let Some(r) = self.action.revisions() {
                set_revisions(w.reborrow().init_revisions(), r);
            }
            if let Action::Inspect { path } | Action::Install { path, .. } = &self.action {
                w.set_path(path.as_str());
            }
            if let Action::BaseEnable { id, .. }
            | Action::Approve { id, .. }
            | Action::WrapperEnable { id, .. }
            | Action::Remove { id, .. } = &self.action
            {
                w.set_package_id(id.as_str());
            }
            if let Action::Install { full_sha256, .. }
            | Action::BaseSelect { full_sha256, .. }
            | Action::BaseEnable { full_sha256, .. }
            | Action::WrapperSelect { full_sha256, .. }
            | Action::Approve { full_sha256, .. }
            | Action::WrapperEnable { full_sha256, .. }
            | Action::Remove { full_sha256, .. } = &self.action
            {
                w.set_full_sha256(full_sha256);
            }
            if let Action::BaseEnable { enabled, .. } | Action::WrapperEnable { enabled, .. } =
                &self.action
            {
                w.set_enabled(*enabled);
            }
            if let Action::Approve { approval: a, .. } = &self.action {
                set_approval(w.reborrow().init_approval(), a);
            }
            if let Action::Page { after, limit, .. } = &self.action {
                if let Some(s) = after {
                    w.set_cursor(s.as_str());
                }
                w.set_limit(*limit);
            }
        }
        frame(&b)
    }
    pub fn decode(input: &[u8]) -> Result<Self> {
        let b = reader(input)?;
        let w = parsed(b.get_root::<wire::request::Reader>())?;
        if w.get_version() != VERSION || parsed(w.get_digest())? != SCHEMA_DIGEST {
            return Err(Error::Contract);
        }
        let id = bytes(w.get_id())?;
        let tag = w.get_action().map_err(|_| Error::Contract)?;
        let r = if matches!(tag, wire::Action::State | wire::Action::Inspect) {
            Revisions::default()
        } else {
            if !w.has_revisions() {
                return Err(Error::Contract);
            }
            get_revisions(parsed(w.get_revisions())?)
        };
        let action = match tag {
            wire::Action::State => Action::State,
            wire::Action::Inspect => Action::Inspect {
                path: text(w.get_path())?,
            },
            wire::Action::Install => Action::Install {
                path: text(w.get_path())?,
                full_sha256: bytes(w.get_full_sha256())?,
                revisions: r,
            },
            wire::Action::BaseSelect => Action::BaseSelect {
                full_sha256: bytes(w.get_full_sha256())?,
                revisions: r,
            },
            wire::Action::BaseEnable => Action::BaseEnable {
                id: text(w.get_package_id())?,
                full_sha256: bytes(w.get_full_sha256())?,
                enabled: w.get_enabled(),
                revisions: r,
            },
            wire::Action::WrapperSelect => Action::WrapperSelect {
                full_sha256: bytes(w.get_full_sha256())?,
                revisions: r,
            },
            wire::Action::Approve => Action::Approve {
                id: text(w.get_package_id())?,
                full_sha256: bytes(w.get_full_sha256())?,
                approval: get_approval(parsed(w.get_approval())?)?,
                revisions: r,
            },
            wire::Action::WrapperEnable => Action::WrapperEnable {
                id: text(w.get_package_id())?,
                full_sha256: bytes(w.get_full_sha256())?,
                enabled: w.get_enabled(),
                revisions: r,
            },
            wire::Action::Remove => Action::Remove {
                id: text(w.get_package_id())?,
                full_sha256: bytes(w.get_full_sha256())?,
                revisions: r,
            },
            wire::Action::Page => {
                let s = text(w.get_cursor())?;
                Action::Page {
                    after: if s.is_empty() { None } else { Some(s) },
                    limit: w.get_limit(),
                    revisions: r,
                }
            }
        };
        let request = Self { id, action };
        request.validate()?;
        if request.encode() != Ok(input.to_vec()) {
            return Err(Error::Contract);
        }
        Ok(request)
    }
}
fn status(v: Status) -> wire::Status {
    match v {
        Status::Ok => wire::Status::Ok,
        Status::Invalid => wire::Status::Invalid,
        Status::Conflict => wire::Status::Conflict,
        Status::Denied => wire::Status::Denied,
        Status::NotFound => wire::Status::NotFound,
        Status::Limit => wire::Status::Limit,
        Status::Storage => wire::Status::Storage,
        Status::Unknown => wire::Status::Unknown,
        Status::Busy => wire::Status::Busy,
        Status::RecoveryRequired => wire::Status::RecoveryRequired,
        Status::OwnerUnavailable => wire::Status::OwnerUnavailable,
        Status::Unsupported => wire::Status::Unsupported,
    }
}
fn get_status(v: wire::Status) -> Status {
    match v {
        wire::Status::Ok => Status::Ok,
        wire::Status::Invalid => Status::Invalid,
        wire::Status::Conflict => Status::Conflict,
        wire::Status::Denied => Status::Denied,
        wire::Status::NotFound => Status::NotFound,
        wire::Status::Limit => Status::Limit,
        wire::Status::Storage => Status::Storage,
        wire::Status::Unknown => Status::Unknown,
        wire::Status::Busy => Status::Busy,
        wire::Status::RecoveryRequired => Status::RecoveryRequired,
        wire::Status::OwnerUnavailable => Status::OwnerUnavailable,
        wire::Status::Unsupported => Status::Unsupported,
    }
}
fn hex(v: &[u8; 32]) -> String {
    v.iter().map(|b| format!("{b:02x}")).collect()
}
fn validate_outcome(request: &Request, v: &Outcome) -> Result<()> {
    if v.status != Status::Ok {
        return if v.body == Body::None {
            Ok(())
        } else {
            Err(Error::Contract)
        };
    }
    match (&request.action, &v.body) {
        (Action::Inspect { .. } | Action::Install { .. }, Body::Review(r)) => {
            review(r)?;
            if let Action::Install { full_sha256, .. } = &request.action
                && *full_sha256 != r.full_sha256
            {
                return Err(Error::Correlation);
            }
        }
        (Action::Page { after, limit, .. }, Body::Page { entries, next }) => {
            if request.action.revisions() != Some(v.revisions) {
                return Err(Error::Correlation);
            }
            if entries.len() > usize::from(*limit) || entries.len() > MAX_PAGE {
                return Err(Error::Limit);
            }
            for e in entries {
                entry(e)?;
                if after
                    .as_ref()
                    .is_some_and(|s| hex(&e.review.full_sha256) <= *s)
                {
                    return Err(Error::Contract);
                }
            }
            if entries
                .windows(2)
                .any(|e| e[0].review.full_sha256 >= e[1].review.full_sha256)
            {
                return Err(Error::Contract);
            }
            if let Some(s) = next
                && (!cursor(s)
                    || entries
                        .last()
                        .is_none_or(|e| hex(&e.review.full_sha256) != *s))
            {
                return Err(Error::Contract);
            }
        }
        (
            Action::State
            | Action::BaseSelect { .. }
            | Action::BaseEnable { .. }
            | Action::WrapperSelect { .. }
            | Action::Approve { .. }
            | Action::WrapperEnable { .. }
            | Action::Remove { .. },
            Body::None,
        ) => {}
        _ => return Err(Error::Contract),
    }
    Ok(())
}
impl Reply {
    pub fn new(request: &Request, outcome: Outcome) -> Result<Self> {
        validate_outcome(request, &outcome)?;
        Ok(Self {
            id: request.id,
            request_sha256: hash(&request.encode()?),
            action: request.action.tag(),
            outcome,
        })
    }
    pub fn encode_for(&self, request: &Request) -> Result<Vec<u8>> {
        if self.id != request.id
            || self.action != request.action.tag()
            || self.request_sha256 != hash(&request.encode()?)
        {
            return Err(Error::Correlation);
        }
        validate_outcome(request, &self.outcome)?;
        let mut b = builder();
        {
            let mut w = b.init_root::<wire::reply::Builder>();
            w.set_version(VERSION);
            w.set_digest(&SCHEMA_DIGEST);
            w.set_id(&self.id);
            w.set_request_sha256(&self.request_sha256);
            w.set_action(wire::Action::try_from(self.action).map_err(|_| Error::Contract)?);
            w.set_status(status(self.outcome.status));
            set_revisions(w.reborrow().init_revisions(), self.outcome.revisions);
            match &self.outcome.body {
                Body::None => w.set_kind(wire::BodyKind::None),
                Body::Review(r) => {
                    w.set_kind(wire::BodyKind::Review);
                    set_review(w.reborrow().init_review(), r);
                }
                Body::Page { entries, next } => {
                    w.set_kind(wire::BodyKind::Page);
                    {
                        let mut list = w.reborrow().init_entries(entries.len() as u32);
                        for (i, e) in entries.iter().enumerate() {
                            set_entry(list.reborrow().get(i as u32), e);
                        }
                    }
                    if let Some(s) = next {
                        w.set_next(s.as_str());
                    }
                }
            }
        }
        frame(&b)
    }
    pub fn decode_for(request: &Request, input: &[u8]) -> Result<Self> {
        let b = reader(input)?;
        let w = parsed(b.get_root::<wire::reply::Reader>())?;
        if w.get_version() != VERSION
            || parsed(w.get_digest())? != SCHEMA_DIGEST
            || !w.has_revisions()
        {
            return Err(Error::Contract);
        }
        let body = match w.get_kind().map_err(|_| Error::Contract)? {
            wire::BodyKind::None => Body::None,
            wire::BodyKind::Review => Body::Review(get_review(parsed(w.get_review())?)?),
            wire::BodyKind::Page => {
                let es = parsed(w.get_entries())?;
                if es.len() as usize > MAX_PAGE {
                    return Err(Error::Limit);
                }
                let entries = es.iter().map(get_entry).collect::<Result<Vec<_>>>()?;
                let s = text(w.get_next())?;
                Body::Page {
                    entries,
                    next: if s.is_empty() { None } else { Some(s) },
                }
            }
        };
        let r = Self {
            id: bytes(w.get_id())?,
            request_sha256: bytes(w.get_request_sha256())?,
            action: w.get_action().map_err(|_| Error::Contract)? as u16,
            outcome: Outcome {
                status: get_status(w.get_status().map_err(|_| Error::Contract)?),
                revisions: get_revisions(parsed(w.get_revisions())?),
                body,
            },
        };
        if r.encode_for(request)? != input {
            return Err(Error::Contract);
        }
        Ok(r)
    }
}
/// One validated call. Lost/malformed/oversized delivery is not a permission to retry an effect.
pub fn respond(target: &mut impl Target, input: &[u8]) -> Result<Vec<u8>> {
    let request = Request::decode(input)?;
    let outcome = target.execute(&request);
    let revisions = outcome.revisions;
    match Reply::new(&request, outcome).and_then(|reply| reply.encode_for(&request)) {
        Ok(frame) => Ok(frame),
        Err(_) => Reply::new(
            &request,
            Outcome::failure(
                if request.is_mutation() {
                    Status::Unknown
                } else {
                    Status::Invalid
                },
                revisions,
            ),
        )?
        .encode_for(&request),
    }
}

/// Bounded rejection for an identifiable invalid frame. No Target is called.
/// The host supplies its current revisions; this never grants execution authority.
/// Correlation hashes the exact rejected bytes, because those bytes are noncanonical.
pub fn reject_frame(input: &[u8], revisions: Revisions) -> Result<Vec<u8>> {
    let b = reader(input)?;
    let w = parsed(b.get_root::<wire::request::Reader>())?;
    if w.get_version() != VERSION || parsed(w.get_digest())? != SCHEMA_DIGEST {
        return Err(Error::Contract);
    }
    let id: [u8; 16] = bytes(w.get_id())?;
    if id == [0; 16] {
        return Err(Error::Invalid);
    }
    let action = w.get_action().map_err(|_| Error::Contract)?;
    let mut b = builder();
    {
        let mut w = b.init_root::<wire::reply::Builder>();
        w.set_version(VERSION);
        w.set_digest(&SCHEMA_DIGEST);
        w.set_id(&id);
        w.set_request_sha256(&hash(input));
        w.set_action(action);
        w.set_status(wire::Status::Invalid);
        set_revisions(w.reborrow().init_revisions(), revisions);
        w.set_kind(wire::BodyKind::None);
    }
    frame(&b)
}
