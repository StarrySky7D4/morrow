//! Runtime handles are intentionally not restorable. Restart invalidates every handle.
use crate::*;
use std::collections::BTreeMap;
use std::sync::{Arc, Weak};
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Binding {
    pub connection: [u8; 32],
    pub session_id: String,
    pub operation_id: String,
    pub generation: u64,
    pub created_at_ms: u64,
    pub expires_at_ms: u64,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Handle {
    pub nonce: [u8; 32],
    pub generation: u64,
}
#[derive(Debug, Clone, Copy)]
pub struct Budget {
    pub max_read_calls: u32,
    pub max_output_bytes: u64,
    pub max_input_bytes: u64,
    pub max_control_calls: u32,
}
impl Default for Budget {
    fn default() -> Self {
        Self {
            max_read_calls: 128,
            max_output_bytes: 1024 * 1024,
            max_input_bytes: 1024 * 1024,
            max_control_calls: 128,
        }
    }
}
/// Rejected MUST mean no effect occurred. All uncertain backend errors are Unknown.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EffectOutcome {
    Accepted,
    Rejected(Error),
    Unknown,
}
pub trait ProcessProvider: Send {
    fn capabilities(&self) -> Capabilities;
    fn read(&mut self, query: ReadQuery) -> Result<OutputPage>;
    fn events(&mut self, query: ReadQuery) -> Result<OutputPage>;
    fn write(&mut self, chunk: &[u8]) -> EffectOutcome;
    fn close_input(&mut self) -> EffectOutcome;
    fn interrupt(&mut self) -> EffectOutcome;
    fn terminate(&mut self) -> EffectOutcome;
    fn resize(&mut self, rows: u16, cols: u16) -> EffectOutcome;
}
struct Receipt {
    digest: [u8; 32],
    outcome: EffectOutcome,
}
struct Entry {
    binding: Binding,
    capabilities: Capabilities,
    budget: Budget,
    provider: Option<Box<dyn ProcessProvider>>,
    receipts: BTreeMap<String, Receipt>,
    revoked: bool,
    uncertain: bool,
    input_closed: bool,
    exited: bool,
    exit_code: Option<i32>,
    closed: bool,
    last_clock: u64,
}
/// Opaque identity for this live trusted process host, independent of its address.
#[derive(Clone)]
pub struct HostIdentity(Weak<()>);
impl HostIdentity {
    pub fn matches(&self, host: &Host) -> bool {
        self.0.upgrade().is_some() && Weak::ptr_eq(&self.0, &Arc::downgrade(&host.identity))
    }
}
#[derive(Default)]
pub struct Host {
    identity: Arc<()>,
    entries: BTreeMap<[u8; 32], Entry>,
    tombstones: BTreeMap<[u8; 32], Binding>,
}
impl Host {
    pub fn identity(&self) -> HostIdentity {
        HostIdentity(Arc::downgrade(&self.identity))
    }
    /// Trusted host only: register the actual StartedProcess, after durable R2 start marking.
    /// Nonce must be fresh cryptographic host entropy. Registration creates no grant.
    pub fn register(
        &mut self,
        binding: Binding,
        nonce: [u8; 32],
        approved: Capabilities,
        budget: Budget,
        provider: Box<dyn ProcessProvider>,
    ) -> Result<Handle> {
        valid_id(&binding.session_id)?;
        valid_id(&binding.operation_id)?;
        if binding.connection == [0; 32]
            || nonce == [0; 32]
            || binding.generation == 0
            || binding.expires_at_ms <= binding.created_at_ms
        {
            return Err(Error::Invalid);
        }
        if self.entries.len() >= MAX_PROCESSES
            || self.entries.len() + self.tombstones.len() >= MAX_TOMBSTONES
            || budget.max_control_calls as usize > MAX_RECEIPTS
            || budget.max_read_calls > MAX_READ_CALLS
            || budget.max_output_bytes > MAX_TOTAL_OUTPUT_BYTES
            || budget.max_input_bytes > MAX_TOTAL_INPUT_BYTES
        {
            return Err(Error::Limit);
        }
        let duplicate = |b: &Binding| {
            b.connection == binding.connection
                && b.operation_id == binding.operation_id
                && b.generation == binding.generation
        };
        if self.entries.contains_key(&nonce)
            || self.tombstones.contains_key(&nonce)
            || self.entries.values().any(|e| duplicate(&e.binding))
            || self.tombstones.values().any(duplicate)
        {
            return Err(Error::Conflict);
        }
        let handle = Handle {
            nonce,
            generation: binding.generation,
        };
        let capabilities = provider.capabilities().intersect(approved);
        let last_clock = binding.created_at_ms;
        self.entries.insert(
            nonce,
            Entry {
                binding,
                capabilities,
                budget,
                provider: Some(provider),
                receipts: BTreeMap::new(),
                revoked: false,
                uncertain: false,
                input_closed: false,
                exited: false,
                exit_code: None,
                closed: false,
                last_clock,
            },
        );
        Ok(handle)
    }
    pub fn revoke(&mut self, handle: Handle) -> Result<()> {
        let entry = self.entry(handle)?;
        entry.revoked = true;
        Ok(())
    }
    /// Remains available after guest revocation/expiry. Does not assert exit or EOF.
    pub fn trusted_cleanup(&mut self, handle: Handle) -> Result<EffectOutcome> {
        let entry = self.entry(handle)?;
        entry.revoked = true;
        Ok(entry.provider.as_mut().ok_or(Error::Unknown)?.terminate())
    }
    pub fn binding(&mut self, handle: Handle) -> Result<Binding> {
        Ok(self.entry(handle)?.binding.clone())
    }
    /// Trusted owner may reconcile after guest expiry, revocation or budget exhaustion.
    pub fn trusted_observe(&mut self, handle: Handle, query: ReadQuery) -> Result<OutputPage> {
        query.validate()?;
        let entry = self.entry(handle)?;
        let page = entry.provider.as_mut().ok_or(Error::Unknown)?.read(query)?;
        observe(entry, &page, query)?;
        Ok(page)
    }
    /// Drop the real provider before freeing the active slot. Nonce/binding stay tombstoned.
    pub fn finish(&mut self, handle: Handle) -> Result<()> {
        let entry = self.entry(handle)?;
        if !entry.exited || !entry.closed {
            return Err(Error::Conflict);
        }
        entry.revoked = true;
        let provider = entry.provider.take().ok_or(Error::Unknown)?;
        drop(provider);
        let entry = self.entries.remove(&handle.nonce).ok_or(Error::NotFound)?;
        self.tombstones.insert(handle.nonce, entry.binding);
        Ok(())
    }
    /// Delivery lost authority after encoding. No new effect is invoked.
    pub fn veto_delivery(&mut self, request: &Request) -> Result<()> {
        if !request.action.is_mutation() {
            return Err(Error::Invalid);
        }
        let digest = request.digest()?;
        let entry = self.entry(Handle {
            nonce: request.handle,
            generation: request.generation,
        })?;
        let receipt = entry
            .receipts
            .get_mut(&request.request_id)
            .ok_or(Error::NotFound)?;
        if receipt.digest != digest {
            return Err(Error::Conflict);
        }
        match receipt.outcome {
            EffectOutcome::Accepted => {
                receipt.outcome = EffectOutcome::Unknown;
                entry.uncertain = true;
                Ok(())
            }
            EffectOutcome::Unknown => Ok(()),
            EffectOutcome::Rejected(_) => Err(Error::Conflict),
        }
    }
    fn entry(&mut self, handle: Handle) -> Result<&mut Entry> {
        let entry = self.entries.get_mut(&handle.nonce).ok_or(Error::NotFound)?;
        if entry.binding.generation != handle.generation {
            return Err(Error::Denied);
        }
        Ok(entry)
    }
    pub fn dispatch(
        &mut self,
        connection: [u8; 32],
        request: &Request,
        mut clock: impl FnMut() -> u64,
        mut authorize: impl FnMut(&Binding) -> bool,
    ) -> Result<ReplyBody> {
        request.validate()?;
        let digest = request.digest()?;
        let entry = self.entry(Handle {
            nonce: request.handle,
            generation: request.generation,
        })?;
        if entry.binding.connection != connection {
            return Err(Error::Denied);
        }
        let now = clock();
        let authorized = authorize(&entry.binding);
        live(entry, now, authorized)?;
        if !entry.capabilities.supports(&request.action) {
            return Err(Error::Unsupported);
        }
        if matches!(request.action, Action::Discover) {
            let now = clock();
            let authorized = authorize(&entry.binding);
            live(entry, now, authorized)?;
            return Ok(ReplyBody::Capabilities(entry.capabilities));
        }
        if let Action::Read(query) | Action::Events(query) = request.action {
            if entry.budget.max_read_calls == 0
                || entry.budget.max_output_bytes < query.max_bytes as u64
            {
                return Err(Error::Limit);
            }
            entry.budget.max_read_calls -= 1;
            entry.budget.max_output_bytes -= query.max_bytes as u64;
            let provider = entry.provider.as_mut().ok_or(Error::Unknown)?;
            let page = if matches!(request.action, Action::Read(_)) {
                provider.read(query)
            } else {
                provider.events(query)
            }?;
            let now = clock();
            let authorized = authorize(&entry.binding);
            live(entry, now, authorized)?;
            observe(entry, &page, query)?;
            return Ok(ReplyBody::Page(page));
        }
        if let Some(receipt) = entry.receipts.get(&request.request_id) {
            if receipt.digest != digest {
                return Err(Error::Conflict);
            }
            let outcome = receipt.outcome;
            let now = clock();
            let authorized = authorize(&entry.binding);
            live(entry, now, authorized)?;
            return effect_reply(outcome);
        }
        if entry.uncertain {
            return Err(Error::Unknown);
        }
        if entry.closed || entry.exited {
            return Err(Error::Closed);
        }
        if entry.input_closed && matches!(request.action, Action::Write(_) | Action::CloseInput) {
            return Err(Error::Closed);
        }
        let input = match &request.action {
            Action::Write(chunk) => chunk.len() as u64,
            _ => 0,
        };
        if entry.budget.max_control_calls == 0
            || entry.budget.max_input_bytes < input
            || entry.receipts.len() >= MAX_RECEIPTS
        {
            return Err(Error::Limit);
        }
        entry.budget.max_control_calls -= 1;
        entry.budget.max_input_bytes -= input;
        // Reserve before entering arbitrary provider code. Panic/unwind also leaves Unknown.
        entry.receipts.insert(
            request.request_id.clone(),
            Receipt {
                digest,
                outcome: EffectOutcome::Unknown,
            },
        );
        entry.uncertain = true;
        let provider = entry.provider.as_mut().ok_or(Error::Unknown)?;
        let outcome = match &request.action {
            Action::Write(chunk) => provider.write(chunk),
            Action::CloseInput => provider.close_input(),
            Action::Interrupt => provider.interrupt(),
            Action::Terminate => provider.terminate(),
            Action::Resize { rows, cols } => provider.resize(*rows, *cols),
            _ => return Err(Error::Invalid),
        };
        let now = clock();
        let authorized = authorize(&entry.binding);
        if live(entry, now, authorized).is_err() {
            return Err(Error::Unknown);
        }
        entry
            .receipts
            .get_mut(&request.request_id)
            .ok_or(Error::Unknown)?
            .outcome = outcome;
        entry.uncertain = outcome == EffectOutcome::Unknown;
        if outcome == EffectOutcome::Accepted && matches!(request.action, Action::CloseInput) {
            entry.input_closed = true
        }
        effect_reply(outcome)
    }
}
fn live(entry: &mut Entry, now: u64, authorized: bool) -> Result<()> {
    if entry.revoked || now < entry.last_clock || now >= entry.binding.expires_at_ms || !authorized
    {
        entry.revoked = true;
        return Err(Error::Denied);
    }
    entry.last_clock = now;
    Ok(())
}
fn observe(entry: &mut Entry, page: &OutputPage, query: ReadQuery) -> Result<()> {
    page.validate_for(query)?;
    if entry.exited && !page.exited
        || entry.closed && !page.closed
        || entry.exit_code.is_some() && entry.exit_code != page.exit_code
    {
        return Err(Error::Invalid);
    }
    entry.exited |= page.exited;
    entry.exit_code = page.exit_code;
    entry.closed |= page.closed;
    Ok(())
}
fn effect_reply(outcome: EffectOutcome) -> Result<ReplyBody> {
    match outcome {
        EffectOutcome::Accepted => Ok(ReplyBody::Accepted),
        EffectOutcome::Rejected(error) => Err(error),
        EffectOutcome::Unknown => Err(Error::Unknown),
    }
}
