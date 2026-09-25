//! Shared application workbench hosted inside the device's browser Worker.
use crate::{Result, Workbench, WorkbenchState, initialize_manager, storage::Storage};
use morrow_core::{audit::{SigningKey, TrustedLog}, plugin_package::{Package, registry::{Registry, RegistryStorage, sqlite::SqliteRegistryStorage}}};
use std::path::Path;
impl Workbench {
    /// The trusted owner loads the locally protected key before calling this.
    /// `create` must come from library selection, never from a failed open retry.
    pub fn open_browser(name: &str, create: bool, trust: TrustedLog, key: SigningKey, package: Option<Package>) -> Result<Self> {
        if name.is_empty() || name.len() > 64 || !name.bytes().all(|c| c.is_ascii_alphanumeric() || matches!(c, b'-' | b'_')) {
            return Err("invalid browser library name".into());
        }
        let path = format!("workbench-{name}.sqlite3");
        let host = Storage::open_browser(Path::new(&path), create, trust, key)?;
        let initialize = || -> Result<_> {
            let path = format!("plugin-registry-{name}.sqlite3");
            let storage = SqliteRegistryStorage::open_opfs(Path::new(&path), true)?;
            if let Some(bundle) = &package { storage.install_package(bundle)?; }
            initialize_manager(Registry::from_storage(Box::new(storage))?, &package)
        };
        Ok(Self {state:WorkbenchState::with_manager(host, initialize(), package)?})
    }
    pub(crate) fn local_state(&self) -> Result<&WorkbenchState> { Ok(&self.state) }
    pub(crate) fn local_state_mut(&mut self) -> Result<&mut WorkbenchState> { Ok(&mut self.state) }
    pub fn maintenance_warning(&self) -> Option<&str> { self.state.maintenance_warning() }
    pub fn finish(&mut self) -> Result<()> { self.state.finish() }
    /// Seal pending audit work without ending the plugin sessions. Admission
    /// must not call finish(), which deliberately shuts those sessions down.
    pub fn browser_checkpoint(&mut self) -> Result<()> { self.state.host.flush_pending() }
    pub fn browser_integrity_check(&self) -> Result<()> { self.state.host.store_local().integrity_check().map_err(Into::into) }
}
