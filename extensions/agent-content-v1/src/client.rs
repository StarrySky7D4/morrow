//! Bounded complete-body assembly for a supplied ContentRef.
//! Successful verification binds bytes to that reference; it proves neither a
//! remote producer's identity nor permission to send those bytes elsewhere.
use crate::{
    Action, ContentRef, Error, MAX_CONTENT_BYTES, MAX_READ_BYTES, Outcome, Reply, Request,
};
use morrow_core::response::{Failure, Outcome as CoreOutcome};
use sha2::{Digest, Sha256};

/// Constructible only after all chunks, their metadata and the complete SHA256
/// have passed read_complete. A reference remains data, never an authorization.
#[derive(Clone, PartialEq, Eq)]
pub struct VerifiedContent {
    reference: ContentRef,
    body: Vec<u8>,
}
impl std::fmt::Debug for VerifiedContent {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("VerifiedContent")
            .field("reference", &self.reference)
            .field("body_length", &self.body.len())
            .finish_non_exhaustive()
    }
}
impl VerifiedContent {
    pub fn reference(&self) -> &ContentRef {
        &self.reference
    }
    pub fn body(&self) -> &[u8] {
        &self.body
    }
    pub fn into_bytes(self) -> Vec<u8> {
        self.body
    }
}
fn rejection(failure: Failure) -> Error {
    if failure == Failure::Limit {
        Error::Limit
    } else {
        Error::Invalid
    }
}

/// Read in ordered 32KiB requests, using IDs prefix-1, prefix-2, and so on.
/// The effective allocation budget is min(max_bytes, MAX_CONTENT_BYTES).
/// Validate every generated ID and the total budget before calling exchange.
/// An empty body still requires one authorized ReadRef request of length one.
/// A denial or any malformed/corrupt response stops immediately without retry;
/// no VerifiedContent or partial body is returned before whole-body validation.
pub fn read_complete(
    reference: &ContentRef,
    request_prefix: &str,
    max_bytes: usize,
    mut exchange: impl FnMut(&Request) -> crate::Result<Vec<u8>>,
) -> crate::Result<VerifiedContent> {
    reference.validate()?;
    let total = usize::try_from(reference.total_length).map_err(|_| Error::Limit)?;
    if total > max_bytes.min(MAX_CONTENT_BYTES as usize) {
        return Err(Error::Limit);
    }
    let requests = total.div_ceil(MAX_READ_BYTES as usize).max(1);
    let suffix_digits = requests.ilog10() as usize + 1;
    let request_id_length = request_prefix
        .len()
        .checked_add(1)
        .and_then(|length| length.checked_add(suffix_digits))
        .ok_or(Error::Invalid)?;
    if request_id_length > 256 {
        return Err(Error::Invalid);
    }
    // Reject an oversized prefix before formatting or allocating its ID.
    // The last numeric suffix is the longest. Its validated length and identical
    // prefix ensure no later ID failure can occur after earlier content escapes.
    Request::new(
        format!("{request_prefix}-{requests}"),
        Action::ReadRef {
            reference: reference.clone(),
            offset: 0,
            length: if total == 0 { 1 } else { MAX_READ_BYTES },
        },
    )?;
    let mut body = Vec::new();
    body.try_reserve_exact(total).map_err(|_| Error::Limit)?;
    for index in 0..requests {
        let offset = body.len();
        let length = if total == 0 {
            1
        } else {
            (total - offset).min(MAX_READ_BYTES as usize) as u32
        };
        let request = Request::new(
            format!("{request_prefix}-{}", index + 1),
            Action::ReadRef {
                reference: reference.clone(),
                offset: offset as u64,
                length,
            },
        )?;
        let bytes = exchange(&request)?;
        let reply = Reply::decode_for(&bytes, &request)?;
        let nested = match reply.outcome {
            Outcome::ReadRef { response } => response,
            Outcome::Rejected { failure } => return Err(rejection(failure)),
            _ => return Err(Error::Correlation),
        };
        let chunk = match crate::core_response(&nested)?.outcome {
            CoreOutcome::ContentChunk(chunk) => chunk,
            CoreOutcome::Rejected(failure) => return Err(rejection(failure)),
            _ => return Err(Error::Correlation),
        };
        // decode_for already binds all reference/range fields. Keep the append
        // invariant local so future codec changes cannot expose a partial value.
        let expected = (total - offset).min(length as usize);
        if chunk.offset != offset as u64 || chunk.bytes.len() != expected {
            return Err(Error::Correlation);
        }
        body.extend_from_slice(&chunk.bytes);
    }
    if body.len() != total || <[u8; 32]>::from(Sha256::digest(&body)) != reference.body_sha256 {
        return Err(Error::Correlation);
    }
    Ok(VerifiedContent {
        reference: reference.clone(),
        body,
    })
}
