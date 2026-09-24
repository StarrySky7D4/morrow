//! Experimental IO v1 guest codec. A valid frame is not a host grant.
//! Callers must reconcile uncertain outcomes; this module never retries.
use crate::{contract, io_capnp as wire};
use capnp::{
    message::{Builder, ReaderOptions},
    serialize,
};
use sha2::{Digest, Sha256};

pub use wire::Status;
pub const VERSION: u16 = 1;
pub const MAX_FRAME_BYTES: usize = 128 * 1024;
pub const MAX_PAYLOAD_BYTES: usize = 64 * 1024;
pub const MAX_REFERENCE_BYTES: usize = 32;
pub const MAX_OPERATION_BYTES: usize = 256;
pub const MAX_ENDPOINT_BYTES: usize = 256;
pub const MAX_CREDENTIAL_BYTES: usize = 4 * 1024;
pub const MAX_SUBMIT_DEADLINE_MS: u64 = 30_000;
pub const MAX_METHOD_BYTES: usize = 16;
pub const MAX_TARGET_BYTES: usize = 2048;
pub const MAX_HEADERS: usize = 64;
pub const MAX_HEADER_NAME_BYTES: usize = 128;
pub const MAX_HEADER_VALUE_BYTES: usize = 8 * 1024;
pub const MAX_HEADER_BYTES: usize = 16 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    Invalid,
    Limit,
    Contract,
    Correlation,
    Unsupported,
}
type Result<T> = std::result::Result<T, Error>;
fn invalid<T>(_: T) -> Error {
    Error::Invalid
}
pub fn schema_digest() -> [u8; 32] {
    contract::IO_DIGEST
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
        return Err(Error::Invalid);
    }
    Ok(message)
}
fn finish(message: &Builder<capnp::message::HeapAllocator>) -> Result<Vec<u8>> {
    let bytes = serialize::write_message_to_words(message);
    if bytes.len() > MAX_FRAME_BYTES {
        Err(Error::Limit)
    } else {
        Ok(bytes)
    }
}
fn reference(bytes: &[u8]) -> Result<[u8; 32]> {
    bytes.try_into().map_err(|_| Error::Invalid)
}
fn opaque(bytes: &[u8], max: usize) -> Result<()> {
    if bytes.is_empty() {
        return Err(Error::Invalid);
    }
    if bytes.len() > max {
        return Err(Error::Limit);
    }
    if bytes.iter().any(|b| b.is_ascii_control()) {
        return Err(Error::Invalid);
    }
    Ok(())
}
fn optional_opaque(bytes: &[u8], max: usize) -> Result<()> {
    if bytes.len() > max {
        return Err(Error::Limit);
    }
    if bytes.iter().any(|b| b.is_ascii_control()) {
        return Err(Error::Invalid);
    }
    Ok(())
}
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
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Header {
    pub name: String,
    pub value: Vec<u8>,
}
fn validate_headers(headers: &[Header], request: bool) -> Result<()> {
    if headers.len() > MAX_HEADERS {
        return Err(Error::Limit);
    }
    let mut total = 0usize;
    for h in headers {
        const TOKEN: &[u8] = b"!#$%&'*+-.^_`|~";
        if h.name.is_empty()
            || !h
                .name
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || TOKEN.contains(&b))
            || h.value
                .iter()
                .any(|&b| (b < 0x20 && b != b'\t') || b == 0x7f)
        {
            return Err(Error::Invalid);
        }
        if h.name.len() > MAX_HEADER_NAME_BYTES || h.value.len() > MAX_HEADER_VALUE_BYTES {
            return Err(Error::Limit);
        }
        if request {
            let name = h.name.to_ascii_lowercase();
            if DENIED_REQUEST_HEADERS.contains(&name.as_str()) || name.starts_with("proxy-") {
                return Err(Error::Invalid);
            }
        }
        total = total
            .saturating_add(h.name.len())
            .saturating_add(h.value.len());
    }
    if total > MAX_HEADER_BYTES {
        Err(Error::Limit)
    } else {
        Ok(())
    }
}
fn decode_headers(
    list: capnp::Result<capnp::struct_list::Reader<'_, wire::header::Owned>>,
) -> Result<Vec<Header>> {
    let list = list.map_err(invalid)?;
    if list.len() as usize > MAX_HEADERS {
        return Err(Error::Limit);
    }
    let mut headers = Vec::with_capacity(list.len() as usize);
    for h in list.iter() {
        let name = h
            .get_name()
            .map_err(invalid)?
            .to_str()
            .map_err(invalid)?
            .to_owned();
        let value = h.get_value().map_err(invalid)?.to_vec();
        headers.push(Header { name, value });
    }
    Ok(headers)
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HttpRequest {
    pub endpoint: Vec<u8>,
    pub method: String,
    pub relative_target: String,
    pub headers: Vec<Header>,
    pub body: Vec<u8>,
    pub credential: Vec<u8>,
}
impl HttpRequest {
    pub fn validate(&self) -> Result<()> {
        opaque(&self.endpoint, MAX_ENDPOINT_BYTES)?;
        optional_opaque(&self.credential, MAX_CREDENTIAL_BYTES)?;
        if self.method.is_empty() || !self.method.bytes().all(|b| b.is_ascii_uppercase()) {
            return Err(Error::Invalid);
        }
        if self.method.len() > MAX_METHOD_BYTES {
            return Err(Error::Limit);
        }
        let t = &self.relative_target;
        if !t.starts_with('/')
            || t.starts_with("//")
            || t.contains('\\')
            || t.contains("://")
            || t.bytes().any(|b| b < 0x21 || b == 0x7f || b == b'#')
        {
            return Err(Error::Invalid);
        }
        if t.len() > MAX_TARGET_BYTES || self.body.len() > MAX_PAYLOAD_BYTES {
            return Err(Error::Limit);
        }
        validate_headers(&self.headers, true)
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SubmissionKind {
    FileRead { reference: [u8; 32] },
    HttpRequest(HttpRequest),
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Submission {
    pub operation_id: Vec<u8>,
    pub deadline_ms: u64,
    pub kind: SubmissionKind,
}
impl Submission {
    pub fn validate(&self) -> Result<()> {
        opaque(&self.operation_id, MAX_OPERATION_BYTES)?;
        if self.deadline_ms > MAX_SUBMIT_DEADLINE_MS {
            return Err(Error::Limit);
        }
        if let SubmissionKind::HttpRequest(http) = &self.kind {
            http.validate()?;
        }
        Ok(())
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Action {
    /// Schema-level submission. Current core dispatch supports HTTP requests;
    /// file reads remain host-unsupported until a runtime implements them.
    Submit(Submission),
    /// Schema-level job polling; current core dispatch reports Unsupported.
    Poll {
        reference: [u8; 32],
    },
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
    /// Schema-level reconciliation; current core dispatch reports Unsupported.
    QueryOperation {
        operation_id: Vec<u8>,
    },
}
impl Action {
    fn validate(&self) -> Result<()> {
        match self {
            Self::Submit(s) => s.validate(),
            Self::Read { limit, .. } if *limit as usize > MAX_PAYLOAD_BYTES => Err(Error::Limit),
            Self::QueryOperation { operation_id } => opaque(operation_id, MAX_OPERATION_BYTES),
            _ => Ok(()),
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Request {
    bytes: Vec<u8>,
    call_id: u64,
    action: Action,
    digest: [u8; 32],
}
impl Request {
    pub fn new(call_id: u64, action: Action) -> Result<Self> {
        action.validate()?;
        let mut message = Builder::new_default();
        let mut root = message.init_root::<wire::request::Builder>();
        root.set_version(VERSION);
        root.set_schema_sha256(&schema_digest());
        root.set_call_id(call_id);
        match &action {
            Action::Submit(s) => {
                let mut submit = root.init_submit();
                submit.set_operation_id(&s.operation_id);
                submit.set_deadline_ms(s.deadline_ms);
                match &s.kind {
                    SubmissionKind::FileRead { reference } => submit.set_file_read(reference),
                    SubmissionKind::HttpRequest(h) => {
                        let mut out = submit.init_http_request();
                        out.set_endpoint(&h.endpoint);
                        out.set_method(&h.method);
                        out.set_relative_target(&h.relative_target);
                        let mut headers = out.reborrow().init_headers(h.headers.len() as u32);
                        for (i, header) in h.headers.iter().enumerate() {
                            let mut entry = headers.reborrow().get(i as u32);
                            entry.set_name(&header.name);
                            entry.set_value(&header.value);
                        }
                        out.set_body(&h.body);
                        out.set_credential(&h.credential);
                    }
                }
            }
            Action::Poll { reference } => root.set_poll(reference),
            Action::Read {
                reference,
                offset,
                limit,
            } => {
                let mut out = root.init_read();
                out.set_reference(reference);
                out.set_offset(*offset);
                out.set_limit(*limit);
            }
            Action::Finish { reference } => root.set_finish(reference),
            Action::Cancel { reference } => root.set_cancel(reference),
            Action::QueryOperation { operation_id } => root.set_query_operation(operation_id),
        }
        Self::decode(&finish(&message)?)
    }
    pub fn decode(bytes: &[u8]) -> Result<Self> {
        let message = read(bytes)?;
        let root = message
            .get_root::<wire::request::Reader>()
            .map_err(invalid)?;
        if root.total_size().map_err(invalid)?.cap_count != 0 {
            return Err(Error::Invalid);
        }
        if root.get_version() != VERSION
            || root.get_schema_sha256().map_err(invalid)? != schema_digest()
        {
            return Err(Error::Contract);
        }
        let action = match root.which().map_err(invalid)? {
            wire::request::Submit(v) => {
                let s = v.map_err(invalid)?;
                let kind = match s.which().map_err(invalid)? {
                    wire::submission::FileRead(v) => SubmissionKind::FileRead {
                        reference: reference(v.map_err(invalid)?)?,
                    },
                    wire::submission::HttpRequest(v) => {
                        let h = v.map_err(invalid)?;
                        SubmissionKind::HttpRequest(HttpRequest {
                            endpoint: h.get_endpoint().map_err(invalid)?.to_vec(),
                            method: h
                                .get_method()
                                .map_err(invalid)?
                                .to_str()
                                .map_err(invalid)?
                                .to_owned(),
                            relative_target: h
                                .get_relative_target()
                                .map_err(invalid)?
                                .to_str()
                                .map_err(invalid)?
                                .to_owned(),
                            headers: decode_headers(h.get_headers())?,
                            body: h.get_body().map_err(invalid)?.to_vec(),
                            credential: h.get_credential().map_err(invalid)?.to_vec(),
                        })
                    }
                    _ => return Err(Error::Unsupported),
                };
                Action::Submit(Submission {
                    operation_id: s.get_operation_id().map_err(invalid)?.to_vec(),
                    deadline_ms: s.get_deadline_ms(),
                    kind,
                })
            }
            wire::request::Poll(v) => Action::Poll {
                reference: reference(v.map_err(invalid)?)?,
            },
            wire::request::Read(v) => {
                let r = v.map_err(invalid)?;
                Action::Read {
                    reference: reference(r.get_reference().map_err(invalid)?)?,
                    offset: r.get_offset(),
                    limit: r.get_limit(),
                }
            }
            wire::request::Finish(v) => Action::Finish {
                reference: reference(v.map_err(invalid)?)?,
            },
            wire::request::Cancel(v) => Action::Cancel {
                reference: reference(v.map_err(invalid)?)?,
            },
            wire::request::QueryOperation(v) => Action::QueryOperation {
                operation_id: v.map_err(invalid)?.to_vec(),
            },
            _ => return Err(Error::Unsupported),
        };
        action.validate()?;
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
    pub fn call_id(&self) -> u64 {
        self.call_id
    }
    pub fn digest(&self) -> [u8; 32] {
        self.digest
    }
    pub fn action(&self) -> &Action {
        &self.action
    }
    pub fn decode_reply(&self, bytes: &[u8]) -> Result<Response> {
        Response::decode(self, bytes)
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Response {
    pub status: Status,
    pub reference: Vec<u8>,
    pub bytes: Vec<u8>,
    pub offset: u64,
    pub eof: bool,
    pub http_status: u16,
    pub headers: Vec<Header>,
    encoded: Vec<u8>,
}
impl Response {
    fn validate(&self, request: &Request) -> Result<()> {
        if self.status == Status::Invalid {
            return Err(Error::Invalid);
        }
        if !self.reference.is_empty() {
            reference(&self.reference)?;
        }
        if self.bytes.len() > MAX_PAYLOAD_BYTES {
            return Err(Error::Limit);
        }
        validate_headers(&self.headers, false)?;
        if self.status != Status::Completed {
            if !self.bytes.is_empty()
                || self.offset != 0
                || !self.eof
                || self.http_status != 0
                || !self.headers.is_empty()
                || (!self.reference.is_empty()
                    && matches!(
                        request.action(),
                        Action::Read { .. }
                            | Action::Finish { .. }
                            | Action::Cancel { .. }
                            | Action::Submit(Submission {
                                kind: SubmissionKind::HttpRequest(_),
                                ..
                            })
                    ))
            {
                return Err(Error::Invalid);
            }
            return Ok(());
        }
        match request.action() {
            Action::Read { offset, limit, .. } => {
                let max = if *limit == 0 {
                    MAX_PAYLOAD_BYTES
                } else {
                    *limit as usize
                };
                if !self.reference.is_empty()
                    || self.http_status != 0
                    || !self.headers.is_empty()
                    || self.offset != *offset
                    || self.bytes.len() > max
                    || self.offset.checked_add(self.bytes.len() as u64).is_none()
                    || (self.bytes.is_empty() && !self.eof)
                {
                    return Err(Error::Invalid);
                }
            }
            Action::Finish { .. } | Action::Cancel { .. } => {
                if !self.reference.is_empty()
                    || !self.bytes.is_empty()
                    || self.offset != 0
                    || !self.eof
                    || self.http_status != 0
                    || !self.headers.is_empty()
                {
                    return Err(Error::Invalid);
                }
            }
            Action::Submit(s) if matches!(s.kind, SubmissionKind::HttpRequest(_)) => {
                if !self.reference.is_empty() || self.offset != 0 || !self.eof {
                    return Err(Error::Invalid);
                }
                if !(100..=599).contains(&self.http_status) {
                    return Err(Error::Invalid);
                }
            }
            Action::Submit(_) | Action::Poll { .. } | Action::QueryOperation { .. } => {
                if self.offset != 0
                    || (self.http_status != 0 && !(100..=599).contains(&self.http_status))
                {
                    return Err(Error::Invalid);
                }
                if self.http_status == 0 && !self.headers.is_empty() {
                    return Err(Error::Invalid);
                }
            }
        }
        Ok(())
    }
    pub fn decode(request: &Request, bytes: &[u8]) -> Result<Self> {
        let message = read(bytes)?;
        let root = message
            .get_root::<wire::response::Reader>()
            .map_err(invalid)?;
        if root.total_size().map_err(invalid)?.cap_count != 0 {
            return Err(Error::Invalid);
        }
        if root.get_version() != VERSION
            || root.get_schema_sha256().map_err(invalid)? != schema_digest()
        {
            return Err(Error::Contract);
        }
        if root.get_call_id() != request.call_id
            || root.get_request_sha256().map_err(invalid)? != request.digest
        {
            return Err(Error::Correlation);
        }
        let result = Self {
            status: root.get_status().map_err(invalid)?,
            reference: root.get_reference().map_err(invalid)?.to_vec(),
            bytes: root.get_bytes().map_err(invalid)?.to_vec(),
            offset: root.get_offset(),
            eof: root.get_eof(),
            http_status: root.get_http_status(),
            headers: decode_headers(root.get_headers())?,
            encoded: bytes.to_vec(),
        };
        result.validate(request)?;
        Ok(result)
    }
    /// Exact validated host frame, suitable for forwarding to the completion boundary.
    pub fn encoded_frame(&self) -> &[u8] {
        &self.encoded
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_http() -> HttpRequest {
        HttpRequest {
            endpoint: b"endpoint-1".to_vec(),
            method: "GET".into(),
            relative_target: "/health".into(),
            headers: vec![Header {
                name: "accept".into(),
                value: b"text/plain".to_vec(),
            }],
            body: vec![],
            credential: vec![],
        }
    }
    // A wire fixture needs explicit control of every correlation and result field.
    #[allow(clippy::too_many_arguments)]
    fn reply(
        request: &Request,
        status: Status,
        reference: &[u8],
        body: &[u8],
        offset: u64,
        eof: bool,
        http_status: u16,
        hash: &[u8],
        call_id: u64,
    ) -> Vec<u8> {
        let mut message = Builder::new_default();
        let mut root = message.init_root::<wire::response::Builder>();
        root.set_version(VERSION);
        root.set_schema_sha256(&schema_digest());
        root.set_call_id(call_id);
        root.set_request_sha256(hash);
        root.set_status(status);
        root.set_reference(reference);
        root.set_bytes(body);
        root.set_offset(offset);
        root.set_eof(eof);
        root.set_http_status(http_status);
        let _ = request;
        finish(&message).unwrap()
    }
    fn ordinary_reply(
        request: &Request,
        status: Status,
        reference: &[u8],
        body: &[u8],
        offset: u64,
        eof: bool,
        http_status: u16,
    ) -> Vec<u8> {
        reply(
            request,
            status,
            reference,
            body,
            offset,
            eof,
            http_status,
            &request.digest(),
            request.call_id(),
        )
    }
    #[test]
    fn request_actions_round_trip_and_unaligned() {
        let op = b"operation-1".to_vec();
        let actions = [
            Action::Submit(Submission {
                operation_id: op.clone(),
                deadline_ms: 5000,
                kind: SubmissionKind::FileRead { reference: [1; 32] },
            }),
            Action::Submit(Submission {
                operation_id: op.clone(),
                deadline_ms: 5000,
                kind: SubmissionKind::HttpRequest(sample_http()),
            }),
            Action::Poll { reference: [2; 32] },
            Action::Read {
                reference: [2; 32],
                offset: 9,
                limit: 32,
            },
            Action::Finish { reference: [2; 32] },
            Action::Cancel { reference: [2; 32] },
            Action::QueryOperation { operation_id: op },
        ];
        for (i, action) in actions.into_iter().enumerate() {
            let request = Request::new(i as u64 + 1, action.clone()).unwrap();
            let mut unaligned = vec![0];
            unaligned.extend_from_slice(request.bytes());
            let parsed = Request::decode(&unaligned[1..]).unwrap();
            assert_eq!(parsed.action(), &action);
            assert_eq!(parsed.bytes(), request.bytes());
            assert_eq!(parsed.digest(), request.digest());
            assert_eq!(Request::decode(&unaligned), Err(Error::Invalid));
        }
    }
    #[test]
    fn receipt_is_bound_to_exact_request_and_preserves_status() {
        let request = Request::new(7, Action::Poll { reference: [2; 32] }).unwrap();
        let pending = ordinary_reply(&request, Status::Pending, &[3; 32], &[], 0, true, 0);
        let decoded = request.decode_reply(&pending).unwrap();
        assert_eq!(decoded.status, Status::Pending);
        assert_eq!(decoded.reference, [3; 32]);
        assert_eq!(decoded.encoded_frame(), pending);
        let unknown = ordinary_reply(&request, Status::OutcomeUnknown, &[], &[], 0, true, 0);
        assert_eq!(
            request.decode_reply(&unknown).unwrap().status,
            Status::OutcomeUnknown
        );
        let bad_hash = reply(&request, Status::Pending, &[], &[], 0, true, 0, &[9; 32], 7);
        assert_eq!(request.decode_reply(&bad_hash), Err(Error::Correlation));
        let bad_call = reply(
            &request,
            Status::Pending,
            &[],
            &[],
            0,
            true,
            0,
            &request.digest(),
            8,
        );
        assert_eq!(request.decode_reply(&bad_call), Err(Error::Correlation));
        let bad_status = ordinary_reply(&request, Status::Invalid, &[], &[], 0, true, 0);
        assert_eq!(request.decode_reply(&bad_status), Err(Error::Invalid));
        let bad_reference = ordinary_reply(&request, Status::Pending, &[0; 31], &[], 0, true, 0);
        assert_eq!(request.decode_reply(&bad_reference), Err(Error::Invalid));
        let bad_eof = ordinary_reply(&request, Status::OutcomeUnknown, &[], &[], 0, false, 0);
        assert_eq!(request.decode_reply(&bad_eof), Err(Error::Invalid));
    }
    #[test]
    fn read_and_http_result_rules() {
        let read = Request::new(
            1,
            Action::Read {
                reference: [4; 32],
                offset: 12,
                limit: 3,
            },
        )
        .unwrap();
        let frame = ordinary_reply(&read, Status::Completed, &[], b"abc", 12, false, 0);
        assert_eq!(read.decode_reply(&frame).unwrap().bytes, b"abc");
        let oversized_chunk = ordinary_reply(&read, Status::Completed, &[], b"abcd", 12, false, 0);
        assert_eq!(read.decode_reply(&oversized_chunk), Err(Error::Invalid));
        let unexpected_reference = ordinary_reply(&read, Status::Denied, &[9; 32], &[], 0, true, 0);
        assert_eq!(
            read.decode_reply(&unexpected_reference),
            Err(Error::Invalid)
        );
        let http = Request::new(
            2,
            Action::Submit(Submission {
                operation_id: b"op".to_vec(),
                deadline_ms: 100,
                kind: SubmissionKind::HttpRequest(sample_http()),
            }),
        )
        .unwrap();
        let remote_404 = ordinary_reply(&http, Status::Completed, &[], b"missing", 0, true, 404);
        let decoded = http.decode_reply(&remote_404).unwrap();
        assert_eq!(decoded.status, Status::Completed);
        assert_eq!(decoded.http_status, 404);
        let missing_http_status = ordinary_reply(&http, Status::Completed, &[], &[], 0, true, 0);
        assert_eq!(http.decode_reply(&missing_http_status), Err(Error::Invalid));
    }
    #[test]
    fn malformed_text_reference_and_limits() {
        let http = Request::new(
            3,
            Action::Submit(Submission {
                operation_id: b"op".to_vec(),
                deadline_ms: 100,
                kind: SubmissionKind::HttpRequest(sample_http()),
            }),
        )
        .unwrap();
        let mut bad_utf8 = http.bytes().to_vec();
        let pos = bad_utf8.windows(4).position(|w| w == b"GET\0").unwrap();
        bad_utf8[pos] = 0xff;
        assert_eq!(Request::decode(&bad_utf8), Err(Error::Invalid));
        let mut message = Builder::new_default();
        let mut root = message.init_root::<wire::request::Builder>();
        root.set_version(VERSION);
        root.set_schema_sha256(&schema_digest());
        root.set_call_id(4);
        root.set_poll(&[1; 31]);
        assert_eq!(
            Request::decode(&finish(&message).unwrap()),
            Err(Error::Invalid)
        );
        assert_eq!(
            Request::decode(&vec![0; MAX_FRAME_BYTES + 1]),
            Err(Error::Limit)
        );
        assert_eq!(
            Request::new(
                5,
                Action::Read {
                    reference: [1; 32],
                    offset: 0,
                    limit: (MAX_PAYLOAD_BYTES + 1) as u32
                }
            ),
            Err(Error::Limit)
        );
        let mut too_many = sample_http();
        too_many.body = vec![0; MAX_PAYLOAD_BYTES + 1];
        assert_eq!(too_many.validate(), Err(Error::Limit));
        let read = Request::new(10, Action::Finish { reference: [1; 32] }).unwrap();
        let mut trailing = read.bytes().to_vec();
        trailing.extend_from_slice(&[0; 8]);
        assert_eq!(Request::decode(&trailing), Err(Error::Invalid));
        let mut wrong_contract = Builder::new_default();
        {
            let mut root = wrong_contract.init_root::<wire::request::Builder>();
            root.set_version(VERSION + 1);
            root.set_schema_sha256(&schema_digest());
            root.set_finish(&[1; 32]);
        }
        assert_eq!(
            Request::decode(&finish(&wrong_contract).unwrap()),
            Err(Error::Contract)
        );
        let mut root = wrong_contract.get_root::<wire::request::Builder>().unwrap();
        root.set_version(VERSION);
        root.set_schema_sha256(&[0; 32]);
        assert_eq!(
            Request::decode(&finish(&wrong_contract).unwrap()),
            Err(Error::Contract)
        );
    }
    #[test]
    fn unimplemented_schema_action_is_explicit() {
        let mut message = Builder::new_default();
        let mut root = message.init_root::<wire::request::Builder>();
        root.set_version(VERSION);
        root.set_schema_sha256(&schema_digest());
        root.set_call_id(9);
        root.init_write();
        assert_eq!(
            Request::decode(&finish(&message).unwrap()),
            Err(Error::Unsupported)
        );
    }
}
