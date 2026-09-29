//! Bounded runtime responses. Projections never replace persistent records.
use crate::{
    Error, Result, content::CardSummary, identity, runtime::*, runtime_capnp, transaction::Receipt,
};
use capnp::message::Builder;
pub use runtime_capnp::Failure;
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Outcome {
    Renamed(Receipt),
    ContentCommitted(Receipt),
    ContentChunk(ContentChunk),
    Summary(CardSummary),
    AttachmentChunk(crate::attachment::AttachmentChunk),
    Rejected(Failure),
    OperationResult {
        card_id: String,
        operation_id: String,
        result: crate::transaction::Lookup,
    },
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
            Outcome::Renamed(v) | Outcome::ContentCommitted(v) => {
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
            Outcome::OperationResult {
                card_id,
                operation_id,
                result,
            } => {
                identity(card_id)?;
                identity(operation_id)?;
                if let crate::transaction::Lookup::Committed(v) = result {
                    identity(&v.event_id)?;
                    if v.card_id != *card_id || v.operation_id != *operation_id || v.revision == 0 {
                        return Err(Error::Integrity);
                    }
                }
            }
            Outcome::AttachmentChunk(v) => v.validate()?,
            Outcome::ContentChunk(v) => v.validate()?,
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
            Outcome::Renamed(v) | Outcome::ContentCommitted(v) => {
                let mut out = if matches!(self.outcome, Outcome::ContentCommitted(_)) {
                    root.init_content_committed()
                } else {
                    root.init_renamed()
                };
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
            Outcome::OperationResult {
                card_id,
                operation_id,
                result,
            } => {
                let mut out = root.init_operation_result();
                out.set_card_id(card_id.as_str());
                out.set_operation_id(operation_id.as_str());
                match result {
                    crate::transaction::Lookup::Absent => out.set_absent_snapshot(()),
                    crate::transaction::Lookup::Committed(v) => {
                        let mut receipt = out.init_locally_committed();
                        receipt.set_operation_id(v.operation_id.as_str());
                        receipt.set_card_id(v.card_id.as_str());
                        receipt.set_revision(v.revision);
                        receipt.set_content_sha256(&v.content_sha256);
                        receipt.set_event_id(v.event_id.as_str());
                    }
                }
            }
            Outcome::AttachmentChunk(v) => {
                let mut out = root.init_attachment_chunk();
                out.set_card_id(v.card_id.as_str());
                out.set_attachment_id(v.attachment_id.as_str());
                out.set_revision(v.revision);
                out.set_offset(v.offset);
                out.set_total_length(v.total_length);
                out.set_content_sha256(&v.content_sha256);
                out.set_bytes(&v.bytes);
            }
            Outcome::ContentChunk(v) => {
                let mut b = root.init_content_chunk();
                b.set_card_id(v.card_id.as_str());
                b.set_revision(v.revision);
                b.set_offset(v.offset);
                b.set_total_length(v.total_length);
                b.set_body_sha256(&v.body_sha256);
                b.set_bytes(&v.bytes);
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
            runtime_capnp::response::ContentChunk(value) => {
                let v = value.map_err(|_| Error::Invalid("body part"))?;
                Outcome::ContentChunk(ContentChunk {
                    card_id: read_text(v.get_card_id())?,
                    revision: v.get_revision(),
                    offset: v.get_offset(),
                    total_length: v.get_total_length(),
                    body_sha256: v
                        .get_body_sha256()
                        .map_err(|_| Error::Integrity)?
                        .try_into()
                        .map_err(|_| Error::Integrity)?,
                    bytes: v.get_bytes().map_err(|_| Error::Integrity)?.to_vec(),
                })
            }
            runtime_capnp::response::ContentCommitted(value) => {
                let v = value.map_err(|_| Error::Invalid("receipt"))?;
                Outcome::ContentCommitted(Receipt {
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
            runtime_capnp::response::OperationResult(value) => {
                let value = value.map_err(|_| Error::Invalid("operation result"))?;
                let result = match value
                    .which()
                    .map_err(|_| Error::Invalid("operation state"))?
                {
                    runtime_capnp::operation_result::AbsentSnapshot(()) => {
                        crate::transaction::Lookup::Absent
                    }
                    runtime_capnp::operation_result::LocallyCommitted(v) => {
                        let v = v.map_err(|_| Error::Invalid("receipt"))?;
                        crate::transaction::Lookup::Committed(Receipt {
                            card_id: read_text(v.get_card_id())?,
                            operation_id: read_text(v.get_operation_id())?,
                            revision: v.get_revision(),
                            event_id: read_text(v.get_event_id())?,
                            content_sha256: v
                                .get_content_sha256()
                                .map_err(|_| Error::Integrity)?
                                .try_into()
                                .map_err(|_| Error::Integrity)?,
                        })
                    }
                };
                Outcome::OperationResult {
                    card_id: read_text(value.get_card_id())?,
                    operation_id: read_text(value.get_operation_id())?,
                    result,
                }
            }
            runtime_capnp::response::AttachmentChunk(value) => {
                let v = value.map_err(|_| Error::Invalid("attachment chunk"))?;
                Outcome::AttachmentChunk(crate::attachment::AttachmentChunk {
                    card_id: read_text(v.get_card_id())?,
                    attachment_id: read_text(v.get_attachment_id())?,
                    revision: v.get_revision(),
                    offset: v.get_offset(),
                    total_length: v.get_total_length(),
                    content_sha256: v
                        .get_content_sha256()
                        .map_err(|_| Error::Integrity)?
                        .try_into()
                        .map_err(|_| Error::Integrity)?,
                    bytes: v.get_bytes().map_err(|_| Error::Integrity)?.to_vec(),
                })
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
        // Missing protected IO material keeps the stable Storage failure code
        // for now; a dedicated runtime error code is deferred to IO-C2.
        Error::EvidenceUnavailable => Failure::Storage,
        _ => Failure::Storage,
    }
}
