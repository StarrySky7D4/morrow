//! Trusted application host. Guests have ordinary package-bound tasks and exact
//! object grants. UI reads remain available when the package is unavailable.
use morrow_core::{
    content::{Attachment, CardRecord},
    content_change::ContentChange,
    lifecycle::GrantKind,
    plugin_package::Package,
    task::{Invocation, Transform},
};
use morrow_plugin_runtime::{
    Limits,
    instance_pool::{Pool, Session},
    manager::Manager,
};
use morrow_workbench_plugin::{Action, Asset, Idea, Request, Response, codec, persistence};
use std::{
    collections::BTreeMap,
    path::Path,
    time::{Instant, SystemTime, UNIX_EPOCH},
};
pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
pub mod capture_provenance;
pub mod credential_control;
mod content_projection;
mod evidence;
pub mod plugin_catalog;
mod preferences_evidence;
mod ui_preferences;
pub mod projection;
pub mod projection_v2;
pub mod query_capture;
pub mod query_plan;
mod query_source;
mod storage;
pub mod transfer;

pub struct Record {
    pub idea: Idea,
    pub revision: u64,
}
pub struct Mutation<'a> {
    pub operation: &'a str,
    pub id: &'a str,
    pub revision: u64,
    pub action: Action,
    pub proposed: Option<Idea>,
    pub text: &'a str,
    pub flag: bool,
}
pub struct Workbench {
    host: storage::Storage,
    plugin: Option<Session>,
    pool: Pool,
    manager: Option<Manager>,
    bundle: Option<Package>,
    catalog: Option<morrow_core::plugin_package::catalog::Catalog>,
    external_ui: Option<plugin_catalog::ExternalUi>,
    external_ui_generation: u64,
    external_ui_closed: std::collections::VecDeque<(String, u64)>,
    ui: Option<morrow_plugin_runtime::inline_ui::InlineUi>,
    ui_generation: u64,
    plugin_warning: Option<String>,
    start: Instant,
    counter: u64,
    query_owner: [u8; 32],
    undo: BTreeMap<String, (u64, u64)>,
    staged: BTreeMap<(String, String), Attachment>,
    transfers: transfer::Transfers,
    pub(crate) capture_transfers: transfer::Transfers,
    capture_scopes: capture_provenance::CaptureScopes,
}
fn command(action: Action) -> Request {
    Request {
        action,
        current: Idea::default(),
        proposed: Idea::default(),
        text: String::new(),
        flag: false,
        now_ms: 0,
        ideas: vec![],
        section: "概览".into(),
        filter: "全部".into(),
        sort: "最近添加".into(),
    }
}
fn now(start: Instant) -> u64 {
    u64::try_from(start.elapsed().as_millis())
        .unwrap_or(u64::MAX - 1)
        .saturating_add(1)
}
impl Workbench {
    pub fn open(path: &Path, package: Option<Package>) -> Result<Self> {
        Self::with_storage(
            storage::Storage::open(path)?,
            path.parent().unwrap_or(Path::new(".")),
            package,
        )
    }
    pub fn open_managed(root: &Path, package: Option<Package>) -> Result<Self> {
        Self::with_storage(storage::Storage::open_managed(root)?, root, package)
    }
    fn with_storage(
        mut host: storage::Storage,
        root: &Path,
        package: Option<Package>,
    ) -> Result<Self> {
        use morrow_core::plugin_package::{catalog::Catalog, registry::Registry};
        let initialize = || -> Result<(Manager, Catalog)> {
            let catalog = Catalog::open(&root.join("plugin-manager/packages"))?;
            if let Some(bundle) = &package {
                catalog.install(bundle)?;
            }
            let reader = Catalog::open(&root.join("plugin-manager/packages"))?;
            let registry = Registry::open(&root.join("plugin-manager/state"), catalog)?;
            let mut manager = Manager::new(registry, Limits::default());
            if let Some(bundle) = &package {
                let id = &bundle.manifest().package_id;
                let first = manager.revision() == 0 && manager.selection(id).is_none();
                manager.select(bundle, manager.revision())?;
                if first {
                    // Explicit bundled-install policy for the existing content API only.
                    // Later upgrades remain disabled until the user approves the new digest.
                    let approved = default_approval()
                        .intersection(bundle.capabilities())
                        .copied()
                        .collect();
                    manager.approve(id, bundle.digest(), approved, manager.revision())?;
                    manager.set_enabled(id, bundle.digest(), true, manager.revision())?;
                }
            }
            Ok((manager, reader))
        };
        let (mut manager, catalog, mut plugin_warning) = match initialize() {
            Ok((manager, catalog)) => (Some(manager), Some(catalog), None),
            Err(error) => (
                None,
                None,
                Some(format!("插件管理暂不可用，已有内容仍可读取。{error}")),
            ),
        };
        let mut pool = Pool::new(&host, Default::default())?;
        let plugin = match (&mut manager, &package) {
            (Some(manager), Some(bundle))
                if manager
                    .selection(&bundle.manifest().package_id)
                    .is_some_and(|s| s.enabled) =>
            {
                let revision = manager.revision();
                match pool.start(
                    manager,
                    &mut host,
                    &bundle.manifest().package_id,
                    &[],
                    revision,
                ) {
                    Ok(instance) => Some(instance),
                    Err(error) => {
                        plugin_warning = Some(format!("插件暂不可用，已有内容仍可读取。{error}"));
                        None
                    }
                }
            }
            _ => None,
        };
        let mut query_owner = [0; 32];
        getrandom::fill(&mut query_owner)?;
        let mut workbench = Self {
            host,
            plugin,
            pool,
            manager,
            bundle: package,
            catalog,
            external_ui: None,
            external_ui_generation: u64::from_le_bytes(query_owner[..8].try_into().unwrap()) >> 1,
            external_ui_closed: Default::default(),
            ui: None,
            ui_generation: 0,
            plugin_warning,
            start: Instant::now(),
            counter: 0,
            query_owner,
            undo: BTreeMap::new(),
            staged: BTreeMap::new(),
            transfers: transfer::Transfers::default(),
            capture_transfers: transfer::Transfers::default(),
            capture_scopes: capture_provenance::CaptureScopes::default(),
        };
        workbench.recover_queries()?;
        Ok(workbench)
    }
    pub fn backup_snapshot(&self, destination: &Path) -> Result<()> {
        self.host.backup_snapshot(destination)
    }
    pub fn backup_key(&self, destination: &Path) -> Result<()> {
        self.host.backup_key(destination)
    }
    pub fn maintenance_warning(&self) -> Option<&str> {
        self.host.warning().or(self.plugin_warning.as_deref())
    }
    pub fn finish(&mut self) -> Result<()> {
        if let Some(mut external) = self.external_ui.take() {
            external.ui.close();
        }
        if let Some(mut ui) = self.ui.take() {
            ui.close();
        }
        let closed = self.pool.close_all(&mut self.host);
        self.plugin = None;
        self.capture_scopes.clear();
        // Disconnecting instances must not prevent a pending durable audit flush attempt.
        let flushed = self.host.flush_pending();
        closed?;
        flushed
    }
    pub fn writable(&self) -> bool {
        self.host.warning().is_none()
            && self.plugin_status().approved
            && self.plugin.as_ref().is_some_and(|session| {
                self.pool.root(session).is_ok_and(|instance| {
                    self.host.connection_phase(instance.connection())
                        == Ok(morrow_core::lifecycle::InstancePhase::Ready)
                })
            })
    }
    fn finish_stopped_session(&mut self) {
        if self
            .plugin
            .as_ref()
            .is_none_or(|session| self.pool.root(session).is_ok())
        {
            return;
        }
        if let Some(mut ui) = self.ui.take() {
            ui.close();
        }
        self.plugin_warning = Some("插件会话已停止；已有内容仍可读取，请显式重新启用插件。".into());
    }
    fn prepare_write(&mut self) -> Result<()> {
        self.host.prepare_write()?;
        if !self.writable() {
            return Err("工作台当前只读，请检查插件权限和内容库状态。".into());
        }
        Ok(())
    }
    fn grant(&mut self, id: &str, kind: GrantKind) -> Result<()> {
        let time = now(self.start);
        self.pool.grant_root(
            &mut self.host,
            self.plugin.as_ref().ok_or("plugin unavailable")?,
            kind,
            id,
            time.saturating_add(5000),
            time,
        )?;
        Ok(())
    }
    fn revoke(&mut self, id: &str, kind: GrantKind) -> Result<()> {
        self.pool.revoke_root(
            &mut self.host,
            self.plugin.as_ref().ok_or("plugin unavailable")?,
            kind,
            id,
        )?;
        Ok(())
    }
    #[cfg(test)]
    fn run(&mut self, input: Request) -> Result<Response> {
        Ok(self.run_observed(input, false)?.0)
    }
    fn run_observed(
        &mut self,
        input: Request,
        capture: bool,
    ) -> Result<(Response, Option<morrow_core::task_evidence::Evidence>)> {
        self.counter = self
            .counter
            .checked_add(1)
            .ok_or("task counter exhausted")?;
        let task = Invocation::new_transform(
            &format!("workbench-{}", self.counter),
            Transform {
                handler: "workbench.command".into(),
                input_type: "morrow.workbench.request.v1".into(),
                output_type: "morrow.workbench.response.v1".into(),
                input: codec::encode_request(&input)?,
            },
        )?;
        let start = self.start;
        let result = if capture {
            self.pool
                .record_transform(
                    self.manager.as_ref().ok_or("plugin manager unavailable")?,
                    &mut self.host,
                    self.plugin.as_ref().ok_or("plugin unavailable")?,
                    &task,
                )
                .map(|captured| {
                    let (report, evidence) = captured.into_parts();
                    (report, Some(evidence))
                })
        } else {
            self.pool
                .run_task(
                    self.manager.as_ref().ok_or("plugin manager unavailable")?,
                    &mut self.host,
                    self.plugin.as_ref().ok_or("plugin unavailable")?,
                    &task,
                    || now(start),
                )
                .map(|report| (report, None))
        };
        self.finish_stopped_session();
        let (r, evidence) = result?;
        if r.execution.outcome != Ok(0) || r.execution.host_calls != 0 || r.response.is_some() {
            return Err(format!("plugin failed: {:?}", r.execution.outcome).into());
        }
        if let Some(f) = r.failure {
            return Err(f.message.into());
        }
        let out = r.output.ok_or("missing plugin output")?;
        if out.type_id != "morrow.workbench.response.v1" {
            return Err("unexpected plugin result".into());
        }
        Ok((codec::decode_response(&out.bytes)?, evidence))
    }
    fn decode(card: &CardRecord) -> Result<Record> {
        let s = card.summary();
        if s.type_id != "org.morrow.idea" || s.format_version != 1 {
            return Err("unsupported content type".into());
        }
        Ok(Record {
            idea: persistence::decode(&s.id, &s.title, &card.body())?,
            revision: s.revision,
        })
    }
    /// Trusted local UI projection, independent of plugin availability. No mutation.
    pub fn read(&self, id: &str) -> Result<Record> {
        Self::decode(&self.host.store_local().card(id)?.ok_or("card not found")?)
    }
    pub fn page(&self, after: &str, limit: u32) -> Result<(Vec<Record>, String)> {
        let ids = self.host.store_local().card_ids_local(after, limit)?;
        let cursor = ids.last().cloned().unwrap_or_default();
        let mut result = Vec::new();
        for id in ids {
            let card = self
                .host
                .store_local()
                .card(&id)?
                .ok_or("card disappeared")?;
            if card.summary().type_id == "org.morrow.idea" {
                result.push(Self::decode(&card)?);
            }
        }
        Ok((result, cursor))
    }
    fn authorized_read(&mut self, id: &str) -> Result<CardRecord> {
        self.grant(id, GrantKind::ReadContent)?;
        let start = self.start;
        let result = self.host.read_content(
            self.pool
                .root(self.plugin.as_ref().ok_or("plugin unavailable")?)?
                .connection(),
            id,
            || now(start),
        );
        self.revoke(id, GrantKind::ReadContent)?;
        Ok(result?)
    }
    /// Called only with a platform-selected stream, never a guest path. A staged
    /// attachment is scoped to the destination card and survives failed commits.
    pub fn import(
        &mut self,
        card: &str,
        name: &str,
        kind: &str,
        reader: &mut impl std::io::Read,
        size: u64,
    ) -> Result<Asset> {
        self.prepare_write()?;
        if !self.writable() {
            return Err("plugin unavailable".into());
        }
        let probe = Idea {
            id: card.into(),
            title: "attachment".into(),
            category: "灵感".into(),
            stage: "待整理".into(),
            assets: vec![Asset {
                id: "pending".into(),
                name: name.into(),
                kind: kind.into(),
                bytes: size,
            }],
            ..Default::default()
        };
        probe.validate()?;
        let clock = i64::try_from(SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis())?;
        let blob = self
            .host
            .store_local_mut()
            .stage_blob(reader, size, None, clock)?;
        let id = format!("asset-{}", blob.id);
        let item = Attachment {
            id: id.clone(),
            display_name: name.into(),
            media_type: match kind {
                "image" => "image/*",
                "gif" => "image/gif",
                "video" => "video/*",
                "audio" => "audio/*",
                _ => "application/octet-stream",
            }
            .into(),
            byte_length: size,
            sha256: blob.sha256,
        };
        self.staged.insert((card.into(), id.clone()), item);
        Ok(Asset {
            id,
            name: name.into(),
            kind: kind.into(),
            bytes: size,
        })
    }
    pub fn create(&mut self, operation: &str, draft: Idea) -> Result<Record> {
        self.prepare_write()?;
        let id = draft.id.clone();
        let mut req = command(Action::Create);
        req.proposed = draft;
        if let Some(record) = self.retry_observed(operation, &id, &req, None)? {
            return Ok(record);
        }
        let (projection, evidence) = self.project_content(operation, &id, req, None, None)?;
        self.commit_projection(&projection, std::slice::from_ref(&evidence))?;
        self.staged.retain(|(card, _), _| card != &id);
        self.read(&id)
    }
    pub fn apply(&mut self, mutation: Mutation<'_>) -> Result<Record> {
        self.prepare_write()?;
        let Mutation {
            operation,
            id,
            revision,
            action,
            proposed,
            text,
            flag,
        } = mutation;
        if matches!(action, Action::Create | Action::Query) {
            return Err("wrong command route".into());
        }
        let mut intent = command(action);
        intent.proposed = proposed.clone().unwrap_or_default();
        intent.text = text.into();
        intent.flag = flag;
        if let Some(record) = self.retry_observed(operation, id, &intent, Some(revision))? {
            return Ok(record);
        }
        let prior = self.authorized_read(id)?;
        let old = Self::decode(&prior)?;
        if old.revision != revision {
            return Err("revision conflict".into());
        }
        let time = now(self.start);
        if action == Action::Restore
            && !self
                .undo
                .get(id)
                .is_some_and(|&(r, deadline)| r == revision && time < deadline)
        {
            return Err("undo expired".into());
        }
        let mut req = command(action);
        req.current = old.idea;
        if action == Action::Edit {
            req.current.description.clear();
            req.current.hypothesis.clear();
            req.current.conclusion.clear();
        }
        req.proposed = proposed.unwrap_or_default();
        req.text = text.into();
        req.flag = flag;
        req.now_ms = time;
        let undo = if action == Action::Restore {
            self.undo
                .get(id)
                .map(|&(revision, deadline)| projection::Undo { revision, deadline })
        } else {
            None
        };
        let (projection, evidence) =
            self.project_content(operation, id, req, Some(&prior), undo)?;
        let receipt = self.commit_projection(&projection, std::slice::from_ref(&evidence))?;
        if action == Action::Delete {
            self.undo
                .insert(id.into(), (receipt.revision, time.saturating_add(8000)));
        } else {
            self.undo.remove(id);
        }
        self.staged.retain(|(card, _), _| card != id);
        self.read(id)
    }
    pub fn export(
        &self,
        card: &str,
        attachment: &str,
        writer: &mut impl std::io::Write,
    ) -> Result<()> {
        self.host
            .store_local()
            .export_attachment_local(card, attachment, writer)?;
        Ok(())
    }
    pub fn read_preferences(&self) -> Result<Option<Vec<u8>>> {
        let Some(card) = self.host.store_local().card("morrow-studio-preferences")? else {
            return Ok(None);
        };
        if card.summary().type_id != "org.morrow.studio" || card.summary().format_version != 1 {
            return Err("preferences type".into());
        }
        let p = morrow_workbench_plugin::preferences::decode_persistent(&card.body())?;
        Ok(Some(morrow_workbench_plugin::preferences::encode_wire(&p)?))
    }
    pub fn save_preferences(&mut self, operation: &str, input: Vec<u8>) -> Result<Vec<u8>> {
        self.save_preferences_observed(operation, input)
    }
    pub fn capture(&mut self, input: Vec<u8>) -> Result<Vec<u8>> {
        self.transform(
            "capture.convert",
            "morrow.capture.request.v1",
            "morrow.capture.response.v1",
            input,
        )
    }
    /// Non-persistent portable service task, still constrained by the registered
    /// handler, package budget and ordinary guest task ABI.
    pub fn service(&mut self, input: Vec<u8>) -> Result<Vec<u8>> {
        self.transform(
            "studio.command",
            "morrow.studio.request.v1",
            "morrow.studio.response.v1",
            input,
        )
    }
    fn transform(
        &mut self,
        handler: &str,
        input_type: &str,
        output_type: &str,
        input: Vec<u8>,
    ) -> Result<Vec<u8>> {
        self.counter = self
            .counter
            .checked_add(1)
            .ok_or("task counter exhausted")?;
        let task = Invocation::new_transform(
            &format!("studio-{}", self.counter),
            Transform {
                handler: handler.into(),
                input_type: input_type.into(),
                output_type: output_type.into(),
                input,
            },
        )?;
        let start = self.start;
        let result = self.pool.run_task(
            self.manager.as_ref().ok_or("plugin manager unavailable")?,
            &mut self.host,
            self.plugin.as_ref().ok_or("plugin unavailable")?,
            &task,
            || now(start),
        );
        self.finish_stopped_session();
        let result = result?;
        if result.execution.outcome != Ok(0)
            || result.execution.host_calls != 0
            || result.response.is_some()
        {
            return Err(format!("studio plugin failed: {:?}", result.execution.outcome).into());
        }
        if let Some(f) = result.failure {
            return Err(f.message.into());
        }
        let out = result.output.ok_or("missing studio output")?;
        if out.type_id != output_type {
            return Err("unexpected studio result".into());
        }
        Ok(out.bytes)
    }
}
impl Drop for Workbench {
    fn drop(&mut self) {
        if let Some(mut ui) = self.ui.take() {
            ui.close();
        }
        let _ = self.pool.close_all(&mut self.host);
        self.plugin = None;
        self.capture_scopes.clear();
    }
}

pub mod protocol;
#[allow(clippy::all)]
pub mod host_capnp {
    include!(concat!(env!("OUT_DIR"), "/host_capnp.rs"));
}

/// Trusted desktop control path; never exposed to a plugin guest.
pub fn restore_key(database: &Path, selected: &Path) -> Result<()> {
    #[cfg(target_os = "windows")]
    {
        morrow_audit::recovery::restore_key(database, selected)
            .map_err(storage::session_message)?;
        Ok(())
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = (database, selected);
        Err("此平台的内容库密钥保护后端尚未接入。".into())
    }
}

pub fn restore_snapshot(archive: &Path, destination: &Path) -> Result<()> {
    #[cfg(target_os = "windows")]
    {
        morrow_audit::snapshot::restore(archive, destination).map_err(storage::session_message)?;
        Ok(())
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = (archive, destination);
        Err("此平台的内容库快照后端尚未接入。".into())
    }
}

/// Trusted startup recovery, serialized with the entire managed host lifetime.
pub fn activate_library(root: &Path, directory: &Path) -> Result<()> {
    #[cfg(target_os = "windows")]
    {
        let mut registry =
            morrow_audit::library::Registry::open(root).map_err(storage::library_message)?;
        registry
            .activate(directory)
            .map_err(storage::library_message)?;
        Ok(())
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = (root, directory);
        Err("此平台的活动内容库管理尚未接入。".into())
    }
}
pub fn restore_active_key(root: &Path, selected: &Path) -> Result<()> {
    #[cfg(target_os = "windows")]
    {
        let registry =
            morrow_audit::library::Registry::open(root).map_err(storage::library_message)?;
        registry
            .restore_key(selected)
            .map_err(storage::library_message)?;
        Ok(())
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = (root, selected);
        Err("此平台的活动内容库管理尚未接入。".into())
    }
}

fn default_approval() -> std::collections::BTreeSet<GrantKind> {
    [
        GrantKind::CreateContent,
        GrantKind::EditContent,
        GrantKind::ReadContent,
    ]
    .into_iter()
    .collect()
}
mod plugin_control;

#[cfg(all(test, target_os = "windows"))]
mod query_archive_tests;

#[cfg(all(test, target_os = "windows"))]
#[path = "../tests/common/mod.rs"]
mod test_common;
