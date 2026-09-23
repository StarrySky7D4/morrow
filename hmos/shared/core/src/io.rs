//! Bounded codec for the existing experimental IO schema. Decoding is not a grant.
//! This first profile handles fixed-resource read/finish/cancel, never a submit or
//! external side effect. Known unsupported operations retain exact correlation.
use crate::{Error, Result, io_capnp as wire};
use capnp::{
    message::{Builder, ReaderOptions},
    serialize,
};
use sha2::{Digest, Sha256};
pub use wire::Status;
pub const VERSION: u16 = 1;
pub const MAX_FRAME_BYTES: usize = 128 * 1024;
pub const MAX_PAYLOAD_BYTES: usize = 64 * 1024;
pub fn schema_digest() -> [u8; 32] {
    crate::plugin_package::io::schema_digest()
}
fn invalid<T>(_: T) -> Error {
    Error::Invalid("IO frame")
}
fn read(bytes: &[u8]) -> Result<capnp::message::Reader<serialize::OwnedSegments>> {
    if bytes.len() > MAX_FRAME_BYTES {
        return Err(Error::Limit);
    }
    let mut remaining = bytes;
    let message = serialize::read_message(
        &mut remaining,
        ReaderOptions {
            traversal_limit_in_words: Some(2 * MAX_FRAME_BYTES / 8),
            nesting_limit: 8,
        },
    )
    .map_err(invalid)?;
    if !remaining.is_empty() {
        return Err(invalid(()));
    }
    Ok(message)
}
fn finish(message: &Builder<capnp::message::HeapAllocator>) -> Result<Vec<u8>> {
    let bytes = serialize::write_message_to_words(message);
    if bytes.len() > MAX_FRAME_BYTES {
        return Err(Error::Limit);
    }
    Ok(bytes)
}
fn reference(value: capnp::Result<capnp::data::Reader<'_>>) -> Result<[u8; 32]> {
    value
        .map_err(invalid)?
        .try_into()
        .map_err(|_| Error::Invalid("IO reference"))
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnsupportedAction {
    Submit,
    Poll,
    Write,
    QueryOperation,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Action {
    Read {
        reference: [u8; 32],
        offset: u64,
        limit: u32,
    },
    Finish {
        reference: [u8; 32],
    },
    Cancel {
        reference: [u8; 32],
    },
    /// Outbound HTTP submission, admitted only by a runtime that owns the
    /// referenced origin, method and credential.
    SubmitHttp(HttpSubmission),
    Unsupported(UnsupportedAction),
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Header {
    pub name: String,
    pub value: Vec<u8>,
}
/// One outbound HTTP submission. The endpoint and credential are opaque
/// host-issued references, never URLs, origins or secrets; the runtime decides
/// whether the referenced origin, method and credential may be used at all.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HttpSubmission {
    pub operation_id: Vec<u8>,
    pub deadline_ms: u64,
    pub endpoint: Vec<u8>,
    pub method: String,
    pub relative_target: String,
    pub headers: Vec<Header>,
    pub body: Vec<u8>,
    pub credential: Vec<u8>,
}
/// Completed outbound HTTP result. A 4xx/5xx remote status is still a completed
/// outcome; only transport-unknown results use `OutcomeUnknown`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HttpOutcome {
    pub status: Status,
    pub http_status: u16,
    pub headers: Vec<Header>,
    pub body: Vec<u8>,
}
pub const MAX_METHOD_BYTES: usize = 16;
pub const MAX_TARGET_BYTES: usize = 2048;
pub const MAX_HEADERS: usize = 64;
pub const MAX_HEADER_NAME_BYTES: usize = 128;
pub const MAX_HEADER_VALUE_BYTES: usize = 8 * 1024;
pub const MAX_HEADER_BYTES: usize = 16 * 1024;
pub const MAX_CREDENTIAL_BYTES: usize = 4 * 1024;
pub const MAX_OPERATION_BYTES: usize = 256;
pub const MAX_ENDPOINT_BYTES: usize = 256;
pub const MAX_SUBMIT_DEADLINE_MS: u64 = crate::plugin_package::io::MAX_DURATION_MS;
/// Hop-by-hop or authority-carrying headers the runtime must own, never the guest.
const DENIED_REQUEST_HEADERS: &[&str] = &[
    "host",
    "authorization",
    "cookie",
    "connection",
    "content-length",
    "transfer-encoding",
    "upgrade",
    "te",
    "trailer",
    "expect",
    "proxy-authorization",
    "proxy-connection",
    "keep-alive",
];
fn http_method(method: &str) -> bool {
    !method.is_empty()
        && method.len() <= MAX_METHOD_BYTES
        && method.bytes().all(|b| b.is_ascii_uppercase())
}
fn http_target(target: &str) -> bool {
    !target.is_empty()
        && target.len() <= MAX_TARGET_BYTES
        && target.starts_with('/')
        && !target.starts_with("//")
        && !target.contains('\\')
        && !target.contains("://")
        && !target.bytes().any(|b| b < 0x21 || b == 0x7f || b == b'#')
}
fn http_header_name(name: &str) -> bool {
    const TOKEN: &[u8] = b"!#$%&'*+-.^_`|~";
    !name.is_empty()
        && name.len() <= MAX_HEADER_NAME_BYTES
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || TOKEN.contains(&b))
}
fn http_header_value(value: &[u8]) -> bool {
    value.len() <= MAX_HEADER_VALUE_BYTES
        && !value.iter().any(|&b| (b < 0x20 && b != b'\t') || b == 0x7f)
}
fn http_reference(bytes: &[u8], limit: usize) -> bool {
    !bytes.is_empty() && bytes.len() <= limit && !bytes.iter().any(|b| b.is_ascii_control())
}
/// Validates one submission before it may be encoded or executed. The method is
/// uppercase-only, the target is origin-form, and the runtime-owned headers are
/// rejected so a guest cannot redirect the pinned origin or framing.
pub fn validate_http_submission(submission: &HttpSubmission) -> Result<()> {
    if !http_reference(&submission.operation_id, MAX_OPERATION_BYTES)
        || !http_reference(&submission.endpoint, MAX_ENDPOINT_BYTES)
        || (submission.credential.len() > MAX_CREDENTIAL_BYTES)
        || submission.credential.iter().any(|b| b.is_ascii_control())
    {
        return Err(Error::Invalid("HTTP submission reference"));
    }
    if submission.deadline_ms > MAX_SUBMIT_DEADLINE_MS {
        return Err(Error::Limit);
    }
    if !http_method(&submission.method) {
        return Err(Error::Invalid("HTTP method"));
    }
    if !http_target(&submission.relative_target) {
        return Err(Error::Invalid("HTTP target"));
    }
    if submission.headers.len() > MAX_HEADERS {
        return Err(Error::Limit);
    }
    let mut total = 0usize;
    for header in &submission.headers {
        if !http_header_name(&header.name) {
            return Err(Error::Invalid("HTTP header name"));
        }
        let name = header.name.to_ascii_lowercase();
        if DENIED_REQUEST_HEADERS.contains(&name.as_str()) || name.starts_with("proxy-") {
            return Err(Error::Invalid("HTTP runtime header"));
        }
        if !http_header_value(&header.value) {
            return Err(Error::Invalid("HTTP header value"));
        }
        total = total.saturating_add(header.name.len() + header.value.len());
    }
    if total > MAX_HEADER_BYTES || submission.body.len() > MAX_PAYLOAD_BYTES {
        return Err(Error::Limit);
    }
    Ok(())
}
/// Validates one HTTP outcome; response headers are remote data and are not
/// subject to the request-side runtime header denial.
pub fn validate_http_outcome(outcome: &HttpOutcome) -> Result<()> {
    if outcome.status == Status::Invalid {
        return Err(Error::Invalid("HTTP outcome status"));
    }
    if outcome.status == Status::Completed {
        if !(100..=599).contains(&outcome.http_status) {
            return Err(Error::Invalid("HTTP status"));
        }
        if outcome.headers.len() > MAX_HEADERS || outcome.body.len() > MAX_PAYLOAD_BYTES {
            return Err(Error::Limit);
        }
        let mut total = 0usize;
        for header in &outcome.headers {
            if !http_header_name(&header.name) || !http_header_value(&header.value) {
                return Err(Error::Invalid("HTTP response header"));
            }
            total = total.saturating_add(header.name.len() + header.value.len());
        }
        if total > MAX_HEADER_BYTES {
            return Err(Error::Limit);
        }
    } else if outcome.http_status != 0 || !outcome.headers.is_empty() || !outcome.body.is_empty() {
        return Err(Error::Invalid("HTTP outcome fields"));
    }
    Ok(())
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Request {
    bytes: Vec<u8>,
    call_id: u64,
    action: Action,
    digest: [u8; 32],
}
impl Request {
    pub fn encode_read(
        call_id: u64,
        reference: &[u8; 32],
        offset: u64,
        limit: u32,
    ) -> Result<Self> {
        if limit as usize > MAX_PAYLOAD_BYTES {
            return Err(Error::Limit);
        }
        Self::encode(
            call_id,
            Action::Read {
                reference: *reference,
                offset,
                limit,
            },
        )
    }
    /// Encodes one outbound HTTP submission. The frame is a request, not a
    /// grant: the runtime still owns the referenced origin, method, credential
    /// and deadline policy.
    pub fn encode_http_submit(call_id: u64, submission: &HttpSubmission) -> Result<Self> {
        validate_http_submission(submission)?;
        let mut message = Builder::new_default();
        let mut root = message.init_root::<wire::request::Builder>();
        root.set_version(VERSION);
        root.set_schema_sha256(&schema_digest());
        root.set_call_id(call_id);
        {
            let mut submit = root.init_submit();
            submit.set_operation_id(&submission.operation_id);
            submit.set_deadline_ms(submission.deadline_ms);
            let mut http = submit.init_http_request();
            http.set_endpoint(&submission.endpoint);
            http.set_method(&submission.method);
            http.set_relative_target(&submission.relative_target);
            let mut headers = http
                .reborrow()
                .init_headers(submission.headers.len() as u32);
            for (index, header) in submission.headers.iter().enumerate() {
                let mut entry = headers.reborrow().get(index as u32);
                entry.set_name(&header.name);
                entry.set_value(&header.value);
            }
            http.set_body(&submission.body);
            http.set_credential(&submission.credential);
        }
        Self::decode(&finish(&message)?)
    }
    pub fn encode_finish(call_id: u64, reference: &[u8; 32]) -> Result<Self> {
        Self::encode(
            call_id,
            Action::Finish {
                reference: *reference,
            },
        )
    }
    pub fn encode_cancel(call_id: u64, reference: &[u8; 32]) -> Result<Self> {
        Self::encode(
            call_id,
            Action::Cancel {
                reference: *reference,
            },
        )
    }
    fn encode(call_id: u64, action: Action) -> Result<Self> {
        let mut message = Builder::new_default();
        let mut root = message.init_root::<wire::request::Builder>();
        root.set_version(VERSION);
        root.set_schema_sha256(&schema_digest());
        root.set_call_id(call_id);
        match action {
            Action::Read {
                reference,
                offset,
                limit,
            } => {
                let mut r = root.init_read();
                r.set_reference(&reference);
                r.set_offset(offset);
                r.set_limit(limit);
            }
            Action::Finish { reference } => root.set_finish(&reference),
            Action::Cancel { reference } => root.set_cancel(&reference),
            Action::SubmitHttp(_) => return Err(Error::Invalid("HTTP submission encoder")),
            Action::Unsupported(_) => return Err(Error::Invalid("unsupported IO encoder")),
        }
        Self::decode(&finish(&message)?)
    }
    pub fn decode(bytes: &[u8]) -> Result<Self> {
        let message = read(bytes)?;
        let root = message
            .get_root::<wire::request::Reader>()
            .map_err(invalid)?;
        if root.total_size().map_err(invalid)?.cap_count != 0 {
            return Err(invalid(()));
        }
        if root.get_version() != VERSION
            || root.get_schema_sha256().map_err(invalid)? != schema_digest()
        {
            return Err(Error::UnsupportedVersion);
        }
        let action = match root.which().map_err(invalid)? {
            wire::request::Read(value) => {
                let r = value.map_err(invalid)?;
                let limit = r.get_limit();
                if limit as usize > MAX_PAYLOAD_BYTES {
                    return Err(Error::Limit);
                }
                Action::Read {
                    reference: reference(r.get_reference())?,
                    offset: r.get_offset(),
                    limit,
                }
            }
            wire::request::Finish(value) => Action::Finish {
                reference: reference(value)?,
            },
            wire::request::Cancel(value) => Action::Cancel {
                reference: reference(value)?,
            },
            wire::request::Submit(value) => {
                // Even unimplemented submissions must reject unknown nested tags.
                let submit = value.map_err(invalid)?;
                match submit.which().map_err(invalid)? {
                    wire::submission::FileRead(v)
                    | wire::submission::FileList(v)
                    | wire::submission::HttpListen(v)
                    | wire::submission::HttpUnpublish(v)
                    | wire::submission::ServiceAccept(v) => {
                        v.map_err(invalid)?;
                        Action::Unsupported(UnsupportedAction::Submit)
                    }
                    wire::submission::FileCreate(v)
                    | wire::submission::FileReplace(v)
                    | wire::submission::FileDelete(v) => {
                        v.map_err(invalid)?;
                        Action::Unsupported(UnsupportedAction::Submit)
                    }
                    wire::submission::HttpRequest(v) => {
                        let http = v.map_err(invalid)?;
                        let mut headers = Vec::new();
                        for header in http.get_headers().map_err(invalid)?.iter() {
                            let name =
                                std::str::from_utf8(header.get_name().map_err(invalid)?.as_bytes())
                                    .map_err(invalid)?
                                    .to_string();
                            headers.push(Header {
                                name,
                                value: header.get_value().map_err(invalid)?.to_vec(),
                            });
                        }
                        let submission = HttpSubmission {
                            operation_id: submit.get_operation_id().map_err(invalid)?.to_vec(),
                            deadline_ms: submit.get_deadline_ms(),
                            endpoint: http.get_endpoint().map_err(invalid)?.to_vec(),
                            method: std::str::from_utf8(
                                http.get_method().map_err(invalid)?.as_bytes(),
                            )
                            .map_err(invalid)?
                            .to_string(),
                            relative_target: std::str::from_utf8(
                                http.get_relative_target().map_err(invalid)?.as_bytes(),
                            )
                            .map_err(invalid)?
                            .to_string(),
                            headers,
                            body: http.get_body().map_err(invalid)?.to_vec(),
                            credential: http.get_credential().map_err(invalid)?.to_vec(),
                        };
                        validate_http_submission(&submission)?;
                        Action::SubmitHttp(submission)
                    }
                    wire::submission::WebSocketConnect(v) => {
                        v.map_err(invalid)?;
                        Action::Unsupported(UnsupportedAction::Submit)
                    }
                    wire::submission::HttpPublish(v) => {
                        v.map_err(invalid)?;
                        Action::Unsupported(UnsupportedAction::Submit)
                    }
                    wire::submission::ServiceReply(v) => {
                        v.map_err(invalid)?;
                        Action::Unsupported(UnsupportedAction::Submit)
                    }
                }
            }
            wire::request::Poll(value) => {
                value.map_err(invalid)?;
                Action::Unsupported(UnsupportedAction::Poll)
            }
            wire::request::Write(value) => {
                value.map_err(invalid)?;
                Action::Unsupported(UnsupportedAction::Write)
            }
            wire::request::QueryOperation(value) => {
                value.map_err(invalid)?;
                Action::Unsupported(UnsupportedAction::QueryOperation)
            }
        };
        Ok(Self {
            bytes: bytes.to_vec(),
            call_id: root.get_call_id(),
            action,
            digest: Sha256::digest(bytes).into(),
        })
    }
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }
    pub fn action(&self) -> &Action {
        &self.action
    }
    pub fn call_id(&self) -> u64 {
        self.call_id
    }
    pub fn digest(&self) -> [u8; 32] {
        self.digest
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Response {
    pub status: Status,
    pub payload: Vec<u8>,
    pub offset: u64,
    pub eof: bool,
}
fn validate_response(
    request: &Request,
    status: Status,
    payload: &[u8],
    offset: u64,
    eof: bool,
) -> Result<()> {
    if payload.len() > MAX_PAYLOAD_BYTES {
        return Err(Error::Limit);
    }
    if status == Status::Invalid {
        return Err(Error::Invalid("invalid IO status"));
    }
    if status != Status::Completed {
        if !payload.is_empty() || offset != 0 || !eof {
            return Err(Error::Invalid("IO status payload"));
        }
        return Ok(());
    }
    match request.action() {
        Action::Read {
            offset: start,
            limit,
            ..
        } => {
            let maximum = if *limit == 0 {
                MAX_PAYLOAD_BYTES
            } else {
                *limit as usize
            };
            if offset != *start
                || payload.len() > maximum
                || offset.checked_add(payload.len() as u64).is_none()
                || (payload.is_empty() && !eof)
            {
                return Err(Error::Invalid("IO read result"));
            }
        }
        Action::Finish { .. } | Action::Cancel { .. } => {
            if !payload.is_empty() || offset != 0 || !eof {
                return Err(Error::Invalid("IO close result"));
            }
        }
        Action::SubmitHttp(_) | Action::Unsupported(_) => {
            return Err(Error::Invalid("unsupported IO success"));
        }
    }
    Ok(())
}
impl Response {
    pub fn encode(
        request: &Request,
        status: Status,
        payload: &[u8],
        offset: u64,
        eof: bool,
    ) -> Result<Vec<u8>> {
        validate_response(request, status, payload, offset, eof)?;
        let mut message = Builder::new_default();
        let mut root = message.init_root::<wire::response::Builder>();
        root.set_version(VERSION);
        root.set_schema_sha256(&schema_digest());
        root.set_call_id(request.call_id());
        root.set_request_sha256(&request.digest());
        root.set_status(status);
        root.set_bytes(payload);
        root.set_offset(offset);
        root.set_eof(eof);
        finish(&message)
    }
    /// Encodes one HTTP outcome bound to its submission frame. A completed
    /// remote 4xx/5xx is still `Completed` with its real `http_status`.
    pub fn encode_http(request: &Request, outcome: &HttpOutcome) -> Result<Vec<u8>> {
        if !matches!(request.action(), Action::SubmitHttp(_)) {
            return Err(Error::Invalid("HTTP outcome request"));
        }
        validate_http_outcome(outcome)?;
        let mut message = Builder::new_default();
        let mut root = message.init_root::<wire::response::Builder>();
        root.set_version(VERSION);
        root.set_schema_sha256(&schema_digest());
        root.set_call_id(request.call_id());
        root.set_request_sha256(&request.digest());
        root.set_status(outcome.status);
        root.set_bytes(&outcome.body);
        root.set_offset(0);
        root.set_eof(true);
        root.set_http_status(outcome.http_status);
        let mut headers = root.init_headers(outcome.headers.len() as u32);
        for (index, header) in outcome.headers.iter().enumerate() {
            let mut entry = headers.reborrow().get(index as u32);
            entry.set_name(&header.name);
            entry.set_value(&header.value);
        }
        finish(&message)
    }
    /// Decodes one HTTP outcome after exact frame binding; response headers are
    /// remote data and are validated for form and size only.
    pub fn decode_http(request: &Request, bytes: &[u8]) -> Result<HttpOutcome> {
        if !matches!(request.action(), Action::SubmitHttp(_)) {
            return Err(Error::Invalid("HTTP outcome request"));
        }
        let message = read(bytes)?;
        let root = message
            .get_root::<wire::response::Reader>()
            .map_err(invalid)?;
        if root.total_size().map_err(invalid)?.cap_count != 0 {
            return Err(invalid(()));
        }
        if root.get_version() != VERSION
            || root.get_schema_sha256().map_err(invalid)? != schema_digest()
        {
            return Err(Error::UnsupportedVersion);
        }
        if root.get_call_id() != request.call_id()
            || root.get_request_sha256().map_err(invalid)? != request.digest()
        {
            return Err(Error::Integrity);
        }
        if !root.get_reference().map_err(invalid)?.is_empty()
            || root.get_offset() != 0
            || !root.get_eof()
        {
            return Err(Error::Invalid("HTTP outcome fields"));
        }
        let mut headers = Vec::new();
        for header in root.get_headers().map_err(invalid)?.iter() {
            let name = std::str::from_utf8(header.get_name().map_err(invalid)?.as_bytes())
                .map_err(invalid)?
                .to_string();
            headers.push(Header {
                name,
                value: header.get_value().map_err(invalid)?.to_vec(),
            });
        }
        let outcome = HttpOutcome {
            status: root.get_status().map_err(invalid)?,
            http_status: root.get_http_status(),
            headers,
            body: root.get_bytes().map_err(invalid)?.to_vec(),
        };
        validate_http_outcome(&outcome)?;
        Ok(outcome)
    }
    pub fn decode(request: &Request, bytes: &[u8]) -> Result<Self> {
        let message = read(bytes)?;
        let root = message
            .get_root::<wire::response::Reader>()
            .map_err(invalid)?;
        if root.total_size().map_err(invalid)?.cap_count != 0 {
            return Err(invalid(()));
        }
        if root.get_version() != VERSION
            || root.get_schema_sha256().map_err(invalid)? != schema_digest()
        {
            return Err(Error::UnsupportedVersion);
        }
        if root.get_call_id() != request.call_id()
            || root.get_request_sha256().map_err(invalid)? != request.digest()
        {
            return Err(Error::Integrity);
        }
        if !root.get_reference().map_err(invalid)?.is_empty()
            || root.get_http_status() != 0
            || !root.get_headers().map_err(invalid)?.is_empty()
        {
            return Err(Error::Invalid("unsupported IO result fields"));
        }
        let status = root.get_status().map_err(invalid)?;
        let payload = root.get_bytes().map_err(invalid)?;
        let offset = root.get_offset();
        let eof = root.get_eof();
        validate_response(request, status, payload, offset, eof)?;
        Ok(Self {
            status,
            payload: payload.to_vec(),
            offset,
            eof,
        })
    }
}
