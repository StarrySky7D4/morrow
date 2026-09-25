//! Device-local audited OPFS storage. The trusted browser owner supplies an
//! already selected identity; this adapter never creates or replaces its key.
use crate::Result;
use morrow_core::{audit::{self, SigningKey, TrustedLog}, dispatch::HostRuntime, store::Store};
use std::{ops::{Deref, DerefMut}, path::Path, sync::atomic::{AtomicBool, Ordering}};
static ACTIVE: AtomicBool = AtomicBool::new(false);
struct Lease;
impl Lease {
    fn acquire() -> Result<Self> {
        ACTIVE.compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst).map_err(|_| "browser workbench already owns a database")?;
        Ok(Self)
    }
}
impl Drop for Lease { fn drop(&mut self) { ACTIVE.store(false, Ordering::SeqCst); } }
pub struct Storage {
    host: HostRuntime,
    trust: TrustedLog,
    key: SigningKey,
    warning: Option<String>,
    _lease: Lease,
}
impl Storage {
    pub(crate) fn open_browser(path: &Path, create: bool, trust: TrustedLog, key: SigningKey) -> Result<Self> {
        if key.verifying_key() != trust.key { return Err("browser library key mismatch".into()); }
        let lease = Lease::acquire()?;
        let store = Store::open_opfs_audited(path, Default::default(), create, trust.clone())?;
        let mut storage = Self {host:HostRuntime::new(store)?,trust,key,warning:None,_lease:lease};
        let _ = storage.flush_pending();
        Ok(storage)
    }
    pub fn warning(&self) -> Option<&str> { self.warning.as_deref() }
    pub fn prepare_write(&mut self) -> Result<()> {
        let (events, bytes) = self.host.store_local().pending_usage()?;
        if self.warning.is_some() || events >= 64 || bytes >= 8 * 1024 * 1024 { self.flush_pending()?; }
        Ok(())
    }
    pub fn flush_pending(&mut self) -> Result<()> {
        let result = audit::sealing::flush(self.host.store_local_mut(), &self.trust, 16,
            |segment| audit::sign(segment, &self.trust, &self.key), || {});
        match result {
            Ok(progress) if !progress.more_pending => { self.warning = None; Ok(()) },
            _ => {
                let message = "内容库维护暂未完成，已有内容仍可读取；请检查设备可用空间后重试保存。";
                self.warning = Some(message.into());
                Err(message.into())
            }
        }
    }
}
impl Deref for Storage { type Target = HostRuntime; fn deref(&self) -> &Self::Target { &self.host } }
impl DerefMut for Storage { fn deref_mut(&mut self) -> &mut Self::Target { &mut self.host } }
