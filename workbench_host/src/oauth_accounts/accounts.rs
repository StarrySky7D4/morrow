//! Store-CAS serializes rotation across original workbench owners. A refresh
//! claim consumes the old secret before a wire request. A lost reply requires
//! fresh sign-in. Logout wins against an outstanding network result.
use super::{Error, Login, Provider, Result};
use morrow_audit::oauth_credentials::{self as protected, Account as Envelope, Tokens};
use morrow_core::{
    outbound_authority::{
        self, Record,
        proto::{self as wire, record::Kind},
    },
    store::Store,
};
use tokio_util::sync::CancellationToken;
use zeroize::Zeroizing;

#[derive(Clone)]
pub struct Account {
    provider: Provider,
    package: [u8; 32],
    reference: [u8; 32],
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum State {
    Ready,
    Refreshing,
    SignInRequired,
    SignedOut,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Status {
    pub reference: [u8; 32],
    pub revision: u64,
    pub state: State,
    pub access_expires_ms: Option<u64>,
}

const MARKER: &[u8] = b"Morrow/OAuth/account-state/v1\0";
fn marker_bytes(binding: [u8; 32], state: State) -> Result<Vec<u8>> {
    let tag = match state {
        State::Refreshing => 1,
        State::SignInRequired => 2,
        State::SignedOut => 3,
        State::Ready => return Err(Error::Invalid),
    };
    Ok([MARKER, &binding, &[tag]].concat())
}
fn marker(record: &Record, binding: [u8; 32], state: State) -> Result<Record> {
    let value = record.value();
    Record::encode(wire::Record {
        schema_version: 1,
        reference: value.reference.clone(),
        revision: value.revision.checked_add(1).ok_or(Error::Limit)?,
        created_ms: value.created_ms,
        expires_ms: value.expires_ms,
        disabled: true,
        kind: Some(Kind::Credential(wire::Credential {
            provider: outbound_authority::WINDOWS_OAUTH_PROVIDER.into(),
            ciphertext: marker_bytes(binding, state)?,
        })),
    })
    .map_err(|_| Error::Storage)
}
fn inspect_marker(record: &Record, binding: [u8; 32]) -> Result<State> {
    let Some(Kind::Credential(blob)) = &record.value().kind else {
        return Err(Error::NotApproved);
    };
    if blob.provider != outbound_authority::WINDOWS_OAUTH_PROVIDER {
        return Err(Error::NotApproved);
    }
    for state in [State::Refreshing, State::SignInRequired, State::SignedOut] {
        if blob.ciphertext == marker_bytes(binding, state)? {
            return Ok(state);
        }
    }
    Err(Error::NotApproved)
}
fn save(store: &mut Store, record: &Record, revision: u64) -> Result<()> {
    store
        .save_outbound_authority_local(record, revision)
        .map_err(|e| match e {
            morrow_core::Error::RevisionConflict => Error::Busy,
            morrow_core::Error::CommitUnknown => Error::Unknown,
            _ => Error::Storage,
        })
}
impl Account {
    /// Rebuilding this binding from a reviewed provider does not authorize a request.
    pub fn bind(provider: Provider, package: [u8; 32], reference: [u8; 32]) -> Result<Self> {
        if package == [0; 32] || reference == [0; 32] {
            return Err(Error::Invalid);
        }
        Ok(Self {
            provider,
            package,
            reference,
        })
    }
    pub fn reference(&self) -> [u8; 32] {
        self.reference
    }
    fn binding(&self, store: &Store) -> Result<[u8; 32]> {
        self.provider.binding(
            &store.tls_store_identity().map_err(|_| Error::Storage)?,
            &self.package,
            &self.reference,
        )
    }
    fn load(&self, store: &Store) -> Result<Record> {
        store
            .load_outbound_authority(&self.reference)
            .map_err(|_| Error::Storage)?
            .ok_or(Error::NotApproved)
    }
    pub fn status(&self, store: &Store, now_ms: u64) -> Result<Status> {
        let record = self.load(store)?;
        self.reviewed_record(store, &record, now_ms)
    }
    pub(crate) fn reviewed_record(
        &self,
        store: &Store,
        record: &Record,
        now_ms: u64,
    ) -> Result<Status> {
        if record.reference() != self.reference {
            return Err(Error::NotApproved);
        }
        let binding = self.binding(store)?;
        let v = record.value();
        let (state, expires) = if v.disabled {
            (inspect_marker(record, binding)?, None)
        } else {
            if protected::binding_hint(record)
                .map_err(|_| Error::NotApproved)?
                .is_some_and(|hint| hint != binding)
            {
                return Err(Error::NotApproved);
            }
            let tokens = protected::open(binding, record).map_err(|_| Error::NotApproved)?;
            if now_ms < v.created_ms || now_ms >= v.expires_ms {
                (State::SignInRequired, None)
            } else {
                (State::Ready, Some(tokens.access_expires_ms))
            }
        };
        Ok(Status {
            reference: self.reference,
            revision: v.revision,
            state,
            access_expires_ms: expires,
        })
    }
    pub(crate) fn signed_out_marker(&self, store: &Store, record: &Record) -> Result<Record> {
        if record.reference() != self.reference {
            return Err(Error::NotApproved);
        }
        marker(record, self.binding(store)?, State::SignedOut)
    }
    pub fn sign_out(
        &self,
        store: &mut Store,
        expected_revision: u64,
        now_ms: u64,
    ) -> Result<Status> {
        let record = self.load(store)?;
        let status = self.reviewed_record(store, &record, now_ms)?;
        if status.revision != expected_revision {
            return Err(Error::Busy);
        }
        if status.state == State::SignedOut {
            return Ok(status);
        }
        let cleared = self.signed_out_marker(store, &record)?;
        save(store, &cleared, expected_revision)?;
        self.status(store, now_ms)
    }
    /// Each caller either reuses the current access generation, waits for the
    /// existing rotation, or acquires the sole right to exchange its old token.
    pub fn begin_refresh(&self, store: &mut Store, now_ms: u64) -> Result<Refresh> {
        let original = self.load(store)?;
        let value = original.value();
        let binding = self.binding(store)?;
        if value.disabled {
            return match inspect_marker(&original, binding)? {
                State::Refreshing => Ok(Refresh::Pending(self.status(store, now_ms)?)),
                _ => Err(Error::Reauthorize),
            };
        }
        original.check_time(now_ms).map_err(|_| Error::Expired)?;
        let tokens = protected::open(binding, &original).map_err(|_| Error::NotApproved)?;
        if now_ms < tokens.access_expires_ms {
            return Ok(Refresh::Current(self.status(store, now_ms)?));
        }
        let claim = marker(&original, binding, State::Refreshing)?;
        save(store, &claim, value.revision)?;
        Ok(Refresh::Start(Rotation {
            account: self.clone(),
            binding,
            claim: claim.value().revision,
            created_ms: value.created_ms,
            expires_ms: value.expires_ms,
            refresh: tokens.refresh,
        }))
    }
    pub(crate) fn interrupt_rotation(&self, store: &mut Store, claim: u64) -> Result<()> {
        let record = self.load(store)?;
        let binding = self.binding(store)?;
        if record.value().revision != claim
            || !record.value().disabled
            || inspect_marker(&record, binding)? != State::Refreshing
        {
            return Err(Error::Busy);
        }
        save(
            store,
            &marker(&record, binding, State::SignInRequired)?,
            claim,
        )
    }
}

pub struct Enrollment {
    pub account: Account,
    login: Login,
    created_ms: u64,
    expires_ms: u64,
    binding: [u8; 32],
}
impl Enrollment {
    pub async fn new(
        store: &Store,
        provider: Provider,
        package: [u8; 32],
        created_ms: u64,
        lifetime_ms: u64,
    ) -> Result<Self> {
        if created_ms == 0 || !(1000..=outbound_authority::MAX_LIFETIME_MS).contains(&lifetime_ms) {
            return Err(Error::Invalid);
        }
        let expires_ms = created_ms.checked_add(lifetime_ms).ok_or(Error::Limit)?;
        let mut reference = [0; 32];
        getrandom::fill(&mut reference).map_err(|_| Error::Unknown)?;
        let account = Account::bind(provider.clone(), package, reference)?;
        if store
            .load_outbound_authority(&reference)
            .map_err(|_| Error::Storage)?
            .is_some()
        {
            return Err(Error::Busy);
        }
        let binding = account.binding(store)?;
        Ok(Self {
            account,
            login: Login::new(provider).await?,
            created_ms,
            expires_ms,
            binding,
        })
    }
    pub fn browser_url(&self) -> url::Url {
        self.login.browser_url()
    }
    pub async fn receive(self, cancel: CancellationToken) -> Result<Update> {
        let tokens = self
            .login
            .complete(self.created_ms, self.expires_ms, cancel)
            .await?;
        Ok(Update {
            account: self.account,
            binding: self.binding,
            expected: 0,
            created_ms: self.created_ms,
            expires_ms: self.expires_ms,
            tokens: Ok(tokens),
        })
    }
}
pub enum Refresh {
    Current(Status),
    Pending(Status),
    Start(Rotation),
}
pub struct Rotation {
    account: Account,
    binding: [u8; 32],
    claim: u64,
    created_ms: u64,
    expires_ms: u64,
    refresh: Zeroizing<String>,
}
impl Rotation {
    pub(crate) fn claim_owner(&self) -> (Account, u64) {
        (self.account.clone(), self.claim)
    }
    pub async fn exchange(self, now_ms: u64, cancel: CancellationToken) -> Update {
        let tokens = self
            .account
            .provider
            .exchange(
                &[
                    ("grant_type", "refresh_token"),
                    ("refresh_token", &self.refresh),
                ],
                Some(&self.refresh),
                now_ms,
                self.expires_ms,
                cancel,
            )
            .await;
        Update {
            account: self.account,
            binding: self.binding,
            expected: self.claim,
            created_ms: self.created_ms,
            expires_ms: self.expires_ms,
            tokens,
        }
    }
}
pub struct Update {
    account: Account,
    binding: [u8; 32],
    expected: u64,
    created_ms: u64,
    expires_ms: u64,
    tokens: Result<Tokens>,
}
impl Update {
    pub fn commit(self, store: &mut Store, now_ms: u64) -> Result<Status> {
        if self.binding != self.account.binding(store)? {
            return Err(Error::NotApproved);
        }
        let revision = self.expected.checked_add(1).ok_or(Error::Limit)?;
        let previous = if self.expected > 0 {
            let previous = self.account.load(store)?;
            if previous.value().revision != self.expected
                || !previous.value().disabled
                || inspect_marker(&previous, self.binding)? != State::Refreshing
            {
                return Err(Error::NotApproved);
            }
            Some(previous)
        } else {
            None
        };
        let record = match self.tokens {
            Ok(tokens)
                if now_ms >= self.created_ms
                    && now_ms < tokens.access_expires_ms
                    && now_ms < self.expires_ms =>
            {
                let resource_scope = protected::resource_scope(
                    &store.tls_store_identity().map_err(|_| Error::Storage)?,
                    &self.account.package,
                    &self
                        .account
                        .provider
                        .resource
                        .origin()
                        .ascii_serialization(),
                );
                protected::seal(
                    Envelope {
                        binding: self.binding,
                        resource_scope,
                        reference: self.account.reference,
                        revision,
                        created_ms: self.created_ms,
                        expires_ms: self.expires_ms,
                    },
                    &tokens,
                )
                .map_err(|_| Error::Storage)?
            }
            failed => {
                if let Some(previous) = previous {
                    save(
                        store,
                        &marker(&previous, self.binding, State::SignInRequired)?,
                        self.expected,
                    )?;
                }
                return Err(failed.err().unwrap_or(Error::Expired));
            }
        };
        save(store, &record, self.expected)?;
        self.account.status(store, now_ms)
    }
}
