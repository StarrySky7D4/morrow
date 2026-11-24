//! Windows account material. The caller binds each account to its original
//! library, package, approved service, granted scopes and opaque account ID.
use crate::{
    credentials::{Error, Result},
    keys,
};
use morrow_core::outbound_authority::{
    self, Record,
    proto::{self as authority, record::Kind},
};
use prost::Message;
use sha2::{Digest, Sha256};
use zeroize::{Zeroize, Zeroizing};

mod proto {
    include!(concat!(env!("OUT_DIR"), "/morrow.oauth_secret.v1.rs"));
}
impl Drop for proto::Secret {
    fn drop(&mut self) {
        self.access_token.zeroize();
        self.refresh_token.zeroize();
    }
}
/// Host-only material. Deliberately does not implement Debug or Serialize.
pub struct Tokens {
    pub access: Zeroizing<String>,
    pub refresh: Zeroizing<String>,
    pub access_expires_ms: u64,
}
#[derive(Clone, Copy)]
pub struct Account {
    pub binding: [u8; 32],
    pub resource_scope: [u8; 32],
    pub reference: [u8; 32],
    pub revision: u64,
    pub created_ms: u64,
    pub expires_ms: u64,
}
fn valid_token(value: &str) -> bool {
    (1..=4096).contains(&value.len()) && value.bytes().all(|b| (0x21..=0x7e).contains(&b))
}
fn valid(a: Account, t: &Tokens) -> bool {
    a.binding != [0; 32]
        && a.resource_scope != [0; 32]
        && a.reference != [0; 32]
        && a.revision > 0
        && a.created_ms > 0
        && a.created_ms < a.expires_ms
        && t.access_expires_ms > a.created_ms
        && t.access_expires_ms <= a.expires_ms
        && valid_token(&t.access)
        && valid_token(&t.refresh)
}
pub fn seal(account: Account, tokens: &Tokens) -> Result<Record> {
    if !valid(account, tokens) {
        return Err(Error::Format);
    }
    let secret = proto::Secret {
        binding: account.binding.to_vec(),
        resource_scope: account.resource_scope.to_vec(),
        reference: account.reference.to_vec(),
        revision: account.revision,
        account_created_ms: account.created_ms,
        account_expires_ms: account.expires_ms,
        access_expires_ms: tokens.access_expires_ms,
        access_token: tokens.access.to_string(),
        refresh_token: tokens.refresh.to_string(),
    };
    let raw = Zeroizing::new(secret.encode_to_vec());
    let protected = keys::protect_oauth(&raw).map_err(|_| Error::Protection)?;
    let ciphertext = [
        b"MOAO1".as_slice(),
        &account.resource_scope,
        &tokens.access_expires_ms.to_le_bytes(),
        &protected,
    ]
    .concat();
    Record::encode(authority::Record {
        schema_version: 1,
        reference: account.reference.to_vec(),
        revision: account.revision,
        created_ms: account.created_ms,
        expires_ms: account.expires_ms,
        disabled: false,
        kind: Some(Kind::Credential(authority::Credential {
            provider: outbound_authority::WINDOWS_OAUTH_PROVIDER.into(),
            ciphertext,
        })),
    })
    .map_err(|_| Error::Format)
}
pub fn resource_scope(library: &[u8; 32], package: &[u8; 32], origin: &str) -> [u8; 32] {
    let mut hash = Sha256::new();
    hash.update(b"Morrow/OAuth/resource/v1\0");
    hash.update(library);
    hash.update(package);
    hash.update(origin.as_bytes());
    hash.finalize().into()
}
/// Bounded inspection for endpoint admission. The original DPAPI plaintext
/// corroborates both fields again before supplying any authorization header.
pub fn access_policy(record: &Record) -> Result<([u8; 32], u64)> {
    let value = record.value();
    let Some(Kind::Credential(blob)) = &value.kind else {
        return Err(Error::Format);
    };
    if value.disabled
        || blob.provider != outbound_authority::WINDOWS_OAUTH_PROVIDER
        || blob.ciphertext.len() <= 45
        || !blob.ciphertext.starts_with(b"MOAO1")
    {
        return Err(Error::Format);
    }
    let scope = blob.ciphertext[5..37]
        .try_into()
        .map_err(|_| Error::Format)?;
    let expires = u64::from_le_bytes(
        blob.ciphertext[37..45]
            .try_into()
            .map_err(|_| Error::Format)?,
    );
    if scope == [0; 32] || expires <= value.created_ms || expires > value.expires_ms {
        return Err(Error::Format);
    }
    Ok((scope, expires))
}
fn material(record: &Record) -> Result<(Account, Tokens)> {
    let value = record.value();
    let (resource_scope, expires) = access_policy(record)?;
    let Some(Kind::Credential(blob)) = &value.kind else {
        return Err(Error::Format);
    };
    let raw = keys::unprotect_oauth(&blob.ciphertext[45..]).map_err(|_| Error::Protection)?;
    if raw.len() > 9216 {
        return Err(Error::Format);
    }
    let mut secret = proto::Secret::decode(raw.as_slice()).map_err(|_| Error::Format)?;
    let account = Account {
        binding: secret
            .binding
            .as_slice()
            .try_into()
            .map_err(|_| Error::Format)?,
        resource_scope,
        reference: record.reference(),
        revision: value.revision,
        created_ms: value.created_ms,
        expires_ms: value.expires_ms,
    };
    if secret.resource_scope != resource_scope
        || secret.access_expires_ms != expires
        || secret.reference != value.reference
        || secret.revision != value.revision
        || secret.account_created_ms != value.created_ms
        || secret.account_expires_ms != value.expires_ms
    {
        return Err(Error::Format);
    }
    let tokens = Tokens {
        access: Zeroizing::new(std::mem::take(&mut secret.access_token)),
        refresh: Zeroizing::new(std::mem::take(&mut secret.refresh_token)),
        access_expires_ms: secret.access_expires_ms,
    };
    if !valid(account, &tokens) {
        return Err(Error::Format);
    }
    Ok((account, tokens))
}
pub fn open(binding: [u8; 32], record: &Record) -> Result<Tokens> {
    let (account, tokens) = material(record)?;
    if account.binding != binding {
        return Err(Error::Format);
    }
    Ok(tokens)
}
pub fn bearer(scope: [u8; 32], record: &Record) -> Result<Zeroizing<String>> {
    let (account, tokens) = material(record)?;
    if account.resource_scope != scope {
        return Err(Error::Format);
    }
    Ok(Zeroizing::new(format!("Bearer {}", tokens.access.as_str())))
}
