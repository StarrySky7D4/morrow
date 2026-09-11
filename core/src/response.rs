//! Bounded runtime responses. Projections never replace persistent records.
use crate::{
    Error, Result, content::CardSummary, identity, runtime::*, runtime_capnp, transaction::Receipt,
};
use capnp::message::Builder;
pub use runtime_capnp::Failure;
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Outcome {
    Renamed(Receipt),
    Summary(CardSummary),
    Rejected(Failure),
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Response {
    pub request_id: String,
    pub outcome: Outcome,
}
impl Response {
    fn validate(&self) -> Result<()> {
        identity(&self.request_id)?;
        match &self.outcome {
            Outcome::Renamed(v) => {
                identity(&v.card_id)?;
                identity(&v.event_id)?;
                if v.operation_id != self.request_id || v.revision == 0 {
                    return Err(Error::Invalid("receipt identity"));
                }
            }
            Outcome::Summary(v) => {
                identity(&v.id)?;
                identity(&v.type_id)?;
                crate::title(&v.title)?;
                if v.revision == 0 || v.format_version == 0 {
                    return Err(Error::Invalid("summary version"));
                }
                if v.preview_text.len() > 16 * 1024 {
                    return Err(Error::Limit);
                }
            }
            Outcome::Rejected(_) => {}
        }
        Ok(())
    }
    pub fn encode(&self) -> Result<Vec<u8>> {
        self.validate()?;
        let mut message = Builder::new_default();
        let mut root = message.init_root::<runtime_capnp::response::Builder>();
        root.set_protocol_version(PROTOCOL_VERSION);
        root.set_runtime_digest(&runtime_digest());
        root.set_content_digest(&content_digest());
        root.set_request_id(self.request_id.as_str());
        match &self.outcome {
            Outcome::Renamed(v) => {
                let mut out = root.init_renamed();
                out.set_operation_id(v.operation_id.as_str());
                out.set_card_id(v.card_id.as_str());
                out.set_revision(v.revision);
                out.set_content_sha256(&v.content_sha256);
                out.set_event_id(v.event_id.as_str());
            }
            Outcome::Summary(v) => {
                let mut out = root.init_summary();
                out.set_protocol_version(PROTOCOL_VERSION);
                out.set_card_id(v.id.as_str());
                out.set_type_id(v.type_id.as_str());
                out.set_format_version(v.format_version);
                out.set_revision(v.revision);
                out.set_title(v.title.as_str());
                out.set_preview_text(v.preview_text.as_str());
            }
            Outcome::Rejected(v) => root.set_rejected(*v),
        }
        bounded_message(&message)
    }
    pub fn decode(bytes: &[u8]) -> Result<Self> {
        let message = read_message(bytes)?;
        let root = message
            .get_root::<runtime_capnp::response::Reader>()
            .map_err(|_| Error::Invalid("response"))?;
        check_contract(
            root.get_protocol_version(),
            root.get_runtime_digest(),
            root.get_content_digest(),
        )?;
        let request_id = read_text(root.get_request_id())?;
        let outcome = match root.which().map_err(|_| Error::Invalid("response kind"))? {
            runtime_capnp::response::Renamed(value) => {
                let v = value.map_err(|_| Error::Invalid("receipt"))?;
                Outcome::Renamed(Receipt {
                    operation_id: read_text(v.get_operation_id())?,
                    card_id: read_text(v.get_card_id())?,
                    revision: v.get_revision(),
                    event_id: read_text(v.get_event_id())?,
                    content_sha256: v
                        .get_content_sha256()
                        .map_err(|_| Error::Integrity)?
                        .try_into()
                        .map_err(|_| Error::Integrity)?,
                })
            }
            runtime_capnp::response::Summary(value) => {
                let v = value.map_err(|_| Error::Invalid("summary"))?;
                if v.get_protocol_version() != PROTOCOL_VERSION {
                    return Err(Error::UnsupportedVersion);
                }
                Outcome::Summary(CardSummary {
                    id: read_text(v.get_card_id())?,
                    type_id: read_text(v.get_type_id())?,
                    format_version: v.get_format_version(),
                    revision: v.get_revision(),
                    title: read_text(v.get_title())?,
                    preview_text: read_text(v.get_preview_text())?,
                })
            }
            runtime_capnp::response::Rejected(value) => {
                Outcome::Rejected(value.map_err(|_| Error::Invalid("failure code"))?)
            }
            _ => return Err(Error::Invalid("response kind")),
        };
        let response = Self {
            request_id,
            outcome,
        };
        response.validate()?;
        Ok(response)
    }
}
#[cfg(any(not(target_arch = "wasm32"), feature = "web-storage"))]
pub(crate) fn failure(error: Error) -> Failure {
    match error {
        Error::NotFound => Failure::NotFound,
        Error::RevisionConflict => Failure::RevisionConflict,
        Error::OperationConflict => Failure::OperationConflict,
        Error::StorageFull | Error::EventCapacity => Failure::Capacity,
        Error::StorageBusy => Failure::Busy,
        Error::CommitUnknown => Failure::CommitUnknown,
        Error::Limit => Failure::Limit,
        Error::Invalid(_) => Failure::Denied,
        _ => Failure::Storage,
    }
}
