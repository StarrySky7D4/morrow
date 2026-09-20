//! Trusted local catalog adapter. Package declarations are not object grants.
use crate::{Result, Workbench, WorkbenchState, now};
use morrow_core::{
    lifecycle::{GrantKind, InstancePhase},
    plugin_package::{Package, catalog, io::IoCapability, registry::Selection},
    task::{Invocation, Transform},
};
use morrow_plugin_runtime::{
    Limits,
    inline_ui::{InlineUi, Reply},
    instance_pool::Session,
    package::PreparedPackage,
};
use std::{collections::BTreeSet, path::Path, time::Duration};
#[derive(Debug)]
pub struct PluginCatalogPage {
    pub revision: u64,
    pub entries: Vec<PluginEntry>,
    pub cursor: String,
}
#[derive(Debug)]
pub struct PluginEntry {
    pub id: String,
    pub name: String,
    pub version: String,
    pub digest: Vec<u8>,
    pub enabled: bool,
    pub builtin: bool,
    pub available: bool,
    pub declared: Vec<String>,
    pub approved: Vec<String>,
    pub declared_io: Vec<String>,
    pub approved_io: Vec<String>,
    pub io_handlers: Vec<String>,
    pub handlers: Vec<PluginHandler>,
    pub dependencies: Vec<String>,
    pub issue: String,
}
#[derive(Debug)]
pub struct PluginHandler {
    pub name: String,
    pub input_type: String,
    pub output_type: String,
    pub max_input_bytes: u32,
    pub max_output_bytes: u32,
}
pub(crate) struct ExternalUi {
    pub(crate) ui: InlineUi,
    session: Session,
    id: String,
    digest: [u8; 32],
}
fn capability(kind: GrantKind) -> &'static str {
    match kind {
        GrantKind::Rename => "rename",
        GrantKind::ReadSummary => "summary",
        GrantKind::QueryOperation => "operation",
        GrantKind::ReadAttachment => "attachment",
        GrantKind::CreateContent => "create-content",
        GrantKind::EditContent => "edit-content",
        GrantKind::ReadContent => "read-content",
    }
}
fn io_capability(kind: IoCapability) -> &'static str {
    match kind {
        IoCapability::FileRead => "file-read",
        IoCapability::FileList => "file-list",
        IoCapability::FileCreate => "file-create",
        IoCapability::FileReplace => "file-replace",
        IoCapability::FileDelete => "file-delete",
        IoCapability::HttpRequest => "http-request",
        IoCapability::HttpListen => "http-listen",
        IoCapability::HttpPublish => "http-publish",
        IoCapability::CredentialUse => "credential-use",
        IoCapability::WebSocketConnect => "websocket-connect",
    }
}
fn io_approval(values: &[String]) -> Result<BTreeSet<IoCapability>> {
    if values.len() > 10 {
        return Err("IO capability decision budget".into());
    }
    let mut set = BTreeSet::new();
    for value in values {
        let kind = (1..=10)
            .filter_map(|n| IoCapability::from_number(n).ok())
            .find(|&kind| io_capability(kind) == value)
            .ok_or("unknown IO capability")?;
        if !set.insert(kind) {
            return Err("duplicate IO capability".into());
        }
    }
    Ok(set)
}
fn approval(values: &[String]) -> Result<BTreeSet<GrantKind>> {
    let mut set = BTreeSet::new();
    for value in values {
        let kind = match value.as_str() {
            "rename" => GrantKind::Rename,
            "summary" => GrantKind::ReadSummary,
            "operation" => GrantKind::QueryOperation,
            "attachment" => GrantKind::ReadAttachment,
            "create-content" => GrantKind::CreateContent,
            "edit-content" => GrantKind::EditContent,
            "read-content" => GrantKind::ReadContent,
            _ => return Err(format!("未知插件权限：{value}").into()),
        };
        if !set.insert(kind) {
            return Err("权限重复。".into());
        }
    }
    Ok(set)
}
impl WorkbenchState {
    fn builtin_id(&self, id: &str) -> bool {
        id == "org.morrow.workbench"
            || self
                .bundle
                .as_ref()
                .is_some_and(|b| b.manifest().package_id == id)
    }
    fn catalog_revision(&self, expected: Option<u64>) -> Result<u64> {
        let r = self.manager.as_ref().ok_or("插件管理不可用。")?.revision();
        if expected.is_some_and(|e| e != r) {
            return Err("插件状态已变化，请刷新后重新确认。".into());
        }
        Ok(r)
    }
    fn external_package(&self, id: &str, digest: &[u8], revision: u64) -> Result<Package> {
        self.catalog_revision(Some(revision))?;
        if self.builtin_id(id) {
            return Err("内置工作台插件请使用原设置入口。".into());
        }
        let s = self
            .manager
            .as_ref()
            .unwrap()
            .selection(id)
            .ok_or("插件未选择或已移除。")?;
        if digest != s.digest {
            return Err("插件摘要已变化，请重新确认。".into());
        }
        Ok(self
            .catalog
            .as_ref()
            .ok_or("插件目录不可用。")?
            .load(s.digest)?)
    }
    fn entry(&self, p: &Package, s: Option<&Selection>) -> PluginEntry {
        let m = p.manifest();
        let selected = s.filter(|s| s.digest == p.digest());
        let issue = if m.dependencies.iter().any(|d| !d.optional) {
            "此插件需要依赖批准；当前管理界面尚不支持配置依赖，不能运行。".into()
        } else {
            match PreparedPackage::new(
                Package::decode(p.archive()).expect("validated immutable package"),
                Limits::default(),
            ) {
                Ok(_) => String::new(),
                Err(e) => format!("插件不能准备执行：{e:?}"),
            }
        };
        PluginEntry {
            id: m.package_id.clone(),
            name: m.display_name.clone(),
            version: m.package_version.clone(),
            digest: p.digest().to_vec(),
            enabled: selected.is_some_and(|s| s.enabled),
            builtin: self.builtin_id(&m.package_id),
            available: issue.is_empty(),
            declared: p
                .capabilities()
                .iter()
                .map(|&k| capability(k).into())
                .collect(),
            approved: selected.map_or(vec![], |s| {
                s.approved.iter().map(|&k| capability(k).into()).collect()
            }),
            declared_io: p
                .io_capabilities()
                .iter()
                .map(|&k| io_capability(k).into())
                .collect(),
            approved_io: selected.map_or(vec![], |s| {
                s.approved_io
                    .iter()
                    .map(|&k| io_capability(k).into())
                    .collect()
            }),
            io_handlers: p
                .io_declaration()
                .map_or_else(Vec::new, |d| d.handlers.clone()),
            handlers: m
                .transform_handlers
                .iter()
                .map(|h| PluginHandler {
                    name: h.handler.clone(),
                    input_type: h.input_type.clone(),
                    output_type: h.output_type.clone(),
                    max_input_bytes: h.max_input_bytes,
                    max_output_bytes: h.max_output_bytes,
                })
                .collect(),
            dependencies: m
                .dependencies
                .iter()
                .map(|d| {
                    format!(
                        "{}: {} ({} → {}), {} [{}]",
                        d.slot,
                        d.handler,
                        d.input_type,
                        d.output_type,
                        d.provider_version,
                        if d.optional { "optional" } else { "required" }
                    )
                })
                .collect(),
            issue,
        }
    }
    pub fn catalog_page(&self, cursor: &str, expected: Option<u64>) -> Result<PluginCatalogPage> {
        let revision = self.catalog_revision(expected)?;
        if !cursor.is_empty() && expected.is_none() {
            return Err("后续目录页必须绑定原状态版本。".into());
        }
        let manager = self.manager.as_ref().unwrap();
        if !cursor.is_empty() && manager.selection(cursor).is_none() {
            return Err("无效插件目录游标。".into());
        }
        let mut entries = vec![];
        let mut next = String::new();
        for s in manager
            .selections()
            .filter(|s| s.package_id.as_str() > cursor)
            .take(3)
        {
            if entries.len() == 2 {
                next = entries.last().map(|e: &PluginEntry| e.id.clone()).unwrap();
                break;
            }
            match self
                .catalog
                .as_ref()
                .ok_or("插件目录不可用。")?
                .load(s.digest)
            {
                Ok(p) => entries.push(self.entry(&p, Some(s))),
                Err(e) => entries.push(PluginEntry {
                    id: s.package_id.clone(),
                    name: s.package_id.clone(),
                    version: String::new(),
                    digest: s.digest.to_vec(),
                    enabled: s.enabled,
                    builtin: self.builtin_id(&s.package_id),
                    available: false,
                    declared: vec![],
                    approved: s.approved.iter().map(|&k| capability(k).into()).collect(),
                    declared_io: vec![],
                    io_handlers: vec![],
                    approved_io: s
                        .approved_io
                        .iter()
                        .map(|&k| io_capability(k).into())
                        .collect(),
                    handlers: vec![],
                    dependencies: vec![],
                    issue: format!("已选包不可用：{e}"),
                }),
            }
        }
        Ok(PluginCatalogPage {
            revision,
            entries,
            cursor: next,
        })
    }
    pub fn inspect_plugin(&self, path: &Path) -> Result<PluginCatalogPage> {
        let revision = self.catalog_revision(None)?;
        let p = catalog::read_file(path)?;
        let entry = self.entry(
            &p,
            self.manager
                .as_ref()
                .unwrap()
                .selection(&p.manifest().package_id),
        );
        Ok(PluginCatalogPage {
            revision,
            entries: vec![entry],
            cursor: String::new(),
        })
    }
    pub fn import_plugin(
        &mut self,
        path: &Path,
        expected_digest: &[u8],
        revision: u64,
    ) -> Result<()> {
        self.catalog_revision(Some(revision))?;
        let p = catalog::read_file(path)?;
        if expected_digest != p.digest() {
            return Err("文件内容已变化，请重新检查插件包。".into());
        }
        if self.builtin_id(&p.manifest().package_id) {
            return Err("禁止外部包覆盖内置工作台插件。".into());
        }
        PreparedPackage::new(
            Package::decode(p.archive()).expect("validated immutable package"),
            Limits::default(),
        )
        .map_err(|e| format!("插件准备失败：{e:?}"))?;
        if let Some(previous) = self
            .manager
            .as_ref()
            .unwrap()
            .selection(&p.manifest().package_id)
            && previous.digest != p.digest()
        {
            let old = self
                .catalog
                .as_ref()
                .ok_or("插件目录不可用。")?
                .load(previous.digest)?;
            let old_version = semver::Version::parse(&old.manifest().package_version)?;
            let new_version = semver::Version::parse(&p.manifest().package_version)?;
            if new_version.cmp_precedence(&old_version) != std::cmp::Ordering::Greater {
                return Err("升级必须使用更高版本；同版本不同内容与降级不能覆盖当前选择。".into());
            }
        }
        self.catalog
            .as_ref()
            .ok_or("插件目录不可用。")?
            .install(&p)?;
        let result = self.manager.as_mut().unwrap().select(&p, revision);
        let cleanup = self.maintain_external();
        result?;
        cleanup
    }
    /// Category approval only: no endpoint, path, credential, object grant or enable decision.
    pub fn configure_external_io(
        &mut self,
        id: &str,
        digest: &[u8],
        revision: u64,
        approved: &[String],
    ) -> Result<()> {
        self.catalog_revision(Some(revision))?;
        if self.builtin_id(id) {
            return Err("built-in plugin requires its dedicated settings".into());
        }
        let allowed = io_approval(approved)?;
        let selection = self
            .manager
            .as_ref()
            .unwrap()
            .selection(id)
            .ok_or("plugin is not selected")?;
        if digest != selection.digest {
            return Err("plugin digest changed".into());
        }
        let exact_digest = selection.digest;
        // Manager preflights the verified declaration before revoking. Empty approval
        // can clear a missing package; expansions still require its original bytes.
        let result = self
            .manager
            .as_mut()
            .unwrap()
            .approve_io(id, exact_digest, allowed, revision);
        let cleanup = self.maintain_external();
        result?;
        cleanup
    }
    pub fn configure_external(
        &mut self,
        id: &str,
        digest: &[u8],
        revision: u64,
        approved: &[String],
        enable: bool,
    ) -> Result<()> {
        self.catalog_revision(Some(revision))?;
        if self.builtin_id(id) {
            return Err("内置工作台插件请使用原设置入口。".into());
        }
        let allowed = approval(approved)?;
        let selection = self
            .manager
            .as_ref()
            .unwrap()
            .selection(id)
            .ok_or("插件未选择或已移除。")?;
        if selection.digest != digest {
            return Err("插件摘要已变化，请重新确认。".into());
        }
        let exact_digest = selection.digest;
        if !enable && selection.approved != allowed {
            return Err("停用不能同时修改权限；请保留当前批准。".into());
        }
        if enable {
            let p = self.external_package(id, digest, revision)?;
            if !allowed.is_subset(p.capabilities()) {
                return Err("批准权限必须属于当前包声明。".into());
            }
            if p.manifest().dependencies.iter().any(|d| !d.optional) {
                return Err("此插件需要依赖批准，当前界面尚不支持配置依赖。".into());
            }
            PreparedPackage::new(p, Limits::default())
                .map_err(|e| format!("插件准备失败：{e:?}"))?;
        }
        let manager = self.manager.as_mut().unwrap();
        let result = if enable {
            manager
                .approve(id, exact_digest, allowed, revision)
                .and_then(|_| manager.set_enabled(id, exact_digest, true, manager.revision()))
        } else {
            manager.set_enabled(id, exact_digest, false, revision)
        };
        let cleanup = self.maintain_external();
        result?;
        cleanup
    }
    pub fn remove_external(&mut self, id: &str, digest: &[u8], revision: u64) -> Result<()> {
        self.catalog_revision(Some(revision))?;
        if self.builtin_id(id) {
            return Err("内置工作台插件请使用原设置入口。".into());
        }
        let selection = self
            .manager
            .as_ref()
            .unwrap()
            .selection(id)
            .ok_or("插件未选择或已移除。")?;
        if selection.digest != digest {
            return Err("插件摘要已变化，请重新确认。".into());
        }
        let result = self.manager.as_mut().unwrap().remove(id, revision);
        let cleanup = self.maintain_external();
        result?;
        cleanup
    }
    pub(crate) fn maintain_external(&mut self) -> Result<()> {
        self.pool.maintain(
            self.manager.as_ref().ok_or("插件管理不可用。")?,
            &mut self.host,
        )?;
        if self
            .external_ui
            .as_ref()
            .is_some_and(|u| self.pool.root(&u.session).is_err())
            && let Some(mut u) = self.external_ui.take()
        {
            u.ui.close();
            if let Err(error) = self.pool.close(&mut self.host, &u.session) {
                self.external_ui = Some(u);
                return Err(error.into());
            }
            self.remember_external_close(u.id, u.ui.generation());
        }
        self.finish_stopped_session();
        Ok(())
    }
    fn start_external(&mut self, id: &str, digest: &[u8], revision: u64) -> Result<Session> {
        let p = self.external_package(id, digest, revision)?;
        if p.manifest().dependencies.iter().any(|d| !d.optional) {
            return Err("此插件需要依赖批准，当前界面尚不支持配置依赖。".into());
        }
        self.maintain_external()?;
        Ok(self.pool.start(
            self.manager.as_mut().unwrap(),
            &mut self.host,
            id,
            &[],
            revision,
        )?)
    }
    #[allow(clippy::too_many_arguments)]
    pub fn run_external_transform(
        &mut self,
        id: &str,
        digest: &[u8],
        revision: u64,
        handler: &str,
        input_type: &str,
        output_type: &str,
        input: &[u8],
    ) -> Result<Vec<u8>> {
        if input.len() > morrow_core::task::MAX_VALUE_BYTES {
            return Err("转换输入超过 64 KiB。".into());
        }
        let invocation = Invocation::new_transform(
            "external-transform",
            Transform {
                handler: handler.into(),
                input_type: input_type.into(),
                output_type: output_type.into(),
                input: input.into(),
            },
        )?;
        let p = self.external_package(id, digest, revision)?;
        p.transform_handler(invocation.transform().unwrap())?;
        let session = self.start_external(id, digest, revision)?;
        let start = self.start;
        let result = self.pool.run_task(
            self.manager.as_ref().unwrap(),
            &mut self.host,
            &session,
            &invocation,
            || now(start),
        );
        let host = &self.host;
        let live = self
            .pool
            .root(&session)
            .is_ok_and(|i| host.connection_phase(i.connection()) == Ok(InstancePhase::Ready));
        let closed = self.pool.close(&mut self.host, &session);
        let report = result?;
        closed?;
        if !live {
            return Err("插件已停止，结果未交付。".into());
        }
        if report.execution.outcome != Ok(0)
            || report.response.is_some()
            || report.execution.host_calls != 0
        {
            return Err(format!("插件转换执行失败：{:?}", report.execution.outcome).into());
        }
        if let Some(failure) = report.failure {
            return Err(format!("插件报告转换失败：{:?} {}", failure.code, failure.message).into());
        }
        Ok(report.output.ok_or("插件未返回转换结果。")?.bytes)
    }
    pub fn external_ui_open(
        &mut self,
        id: &str,
        digest: &[u8],
        revision: u64,
        seed: &str,
    ) -> Result<Reply> {
        self.maintain_external()?;
        if self.external_ui.is_some() {
            return Err("已有外部插件表单，请先关闭。".into());
        }
        self.external_ui_generation = self
            .external_ui_generation
            .checked_add(1)
            .ok_or("界面代次耗尽")?;
        let generation = self.external_ui_generation;
        let view = format!(
            "external-{}",
            self.query_owner
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect::<String>()
        );
        let session = self.start_external(id, digest, revision)?;
        let result = (|| -> Result<(InlineUi, Reply)> {
            let instance = self.pool.root(&session)?;
            let mut ui = InlineUi::new(
                instance.package(),
                &self.host,
                instance.connection(),
                &view,
                generation,
                Duration::from_secs(3),
            )?;
            let reply = ui.open(
                instance.package(),
                &mut self.host,
                instance.connection(),
                seed,
            )?;
            Ok((ui, reply))
        })();
        match result {
            Ok((ui, reply)) => {
                self.external_ui = Some(ExternalUi {
                    ui,
                    session,
                    id: id.into(),
                    digest: digest.try_into()?,
                });
                self.check_external_reply(&reply)?;
                Ok(reply)
            }
            Err(e) => {
                self.pool.close(&mut self.host, &session)?;
                Err(e)
            }
        }
    }
    pub fn external_ui_event(&mut self, id: &str, generation: u64, bytes: &[u8]) -> Result<Reply> {
        self.maintain_external()?;
        let u = self.external_ui.as_mut().ok_or("外部插件表单已关闭。")?;
        if u.id != id || u.ui.generation() != generation {
            return Err("外部插件表单已更换。".into());
        }
        let instance = self.pool.root(&u.session)?;
        if instance.package().package().digest() != u.digest {
            return Err("插件包已更换。".into());
        }
        let reply = u.ui.event(
            instance.package(),
            &mut self.host,
            instance.connection(),
            bytes,
        )?;
        self.check_external_reply(&reply)?;
        Ok(reply)
    }
    fn check_external_reply(&mut self, reply: &Reply) -> Result<()> {
        use morrow_plugin_runtime::{Fault, inline_ui::Failure};
        if matches!(
            reply.failure,
            Some(Failure::Execution(
                Fault::Trap | Fault::Limits | Fault::TaskProtocol
            ))
        ) && let Some(u) = &self.external_ui
            && let Ok(i) = self.pool.root(&u.session)
        {
            i.stop();
        }
        self.maintain_external()?;
        if reply.failure.is_none() {
            let u = self
                .external_ui
                .as_ref()
                .ok_or("插件已停止，表单未交付。")?;
            let i = self.pool.root(&u.session)?;
            if self.host.connection_phase(i.connection()) != Ok(InstancePhase::Ready) {
                return Err("插件已撤权，表单未交付。".into());
            }
        }
        Ok(())
    }
    fn remember_external_close(&mut self, id: String, generation: u64) {
        // Only completed close / confirmed retired roots enter this bounded retry cache.
        if self.external_ui_closed.len() == 64 {
            self.external_ui_closed.pop_front();
        }
        self.external_ui_closed.push_back((id, generation));
    }
    pub fn external_ui_close(&mut self, id: &str, generation: u64) -> Result<()> {
        if self
            .external_ui_closed
            .iter()
            .any(|(closed_id, closed_generation)| {
                closed_id == id && *closed_generation == generation
            })
        {
            return Ok(());
        }
        let u = self.external_ui.as_ref().ok_or("外部插件表单已关闭。")?;
        if u.id != id || u.ui.generation() != generation {
            return Err("外部插件表单已更换。".into());
        }
        let mut u = self.external_ui.take().unwrap();
        u.ui.close();
        if let Err(error) = self.pool.close(&mut self.host, &u.session) {
            self.external_ui = Some(u);
            return Err(error.into());
        }
        self.remember_external_close(u.id, generation);
        Ok(())
    }
}

impl Workbench {
    pub fn catalog_page(&self, cursor: &str, expected: Option<u64>) -> Result<PluginCatalogPage> {
        self.local_state()?.catalog_page(cursor, expected)
    }
    pub fn inspect_plugin(&self, path: &Path) -> Result<PluginCatalogPage> {
        self.local_state()?.inspect_plugin(path)
    }
    pub fn import_plugin(
        &mut self,
        path: &Path,
        expected_digest: &[u8],
        revision: u64,
    ) -> Result<()> {
        self.local_state_mut()?
            .import_plugin(path, expected_digest, revision)
    }
    pub fn configure_external_io(
        &mut self,
        id: &str,
        digest: &[u8],
        revision: u64,
        approved: &[String],
    ) -> Result<()> {
        self.local_state_mut()?
            .configure_external_io(id, digest, revision, approved)
    }
    pub fn configure_external(
        &mut self,
        id: &str,
        digest: &[u8],
        revision: u64,
        approved: &[String],
        enable: bool,
    ) -> Result<()> {
        self.local_state_mut()?
            .configure_external(id, digest, revision, approved, enable)
    }
    pub fn remove_external(&mut self, id: &str, digest: &[u8], revision: u64) -> Result<()> {
        self.local_state_mut()?
            .remove_external(id, digest, revision)
    }
    #[allow(clippy::too_many_arguments)]
    pub fn run_external_transform(
        &mut self,
        id: &str,
        digest: &[u8],
        revision: u64,
        handler: &str,
        input_type: &str,
        output_type: &str,
        input: &[u8],
    ) -> Result<Vec<u8>> {
        self.local_state_mut()?.run_external_transform(
            id,
            digest,
            revision,
            handler,
            input_type,
            output_type,
            input,
        )
    }
    pub fn external_ui_open(
        &mut self,
        id: &str,
        digest: &[u8],
        revision: u64,
        seed: &str,
    ) -> Result<Reply> {
        self.local_state_mut()?
            .external_ui_open(id, digest, revision, seed)
    }
    pub fn external_ui_event(&mut self, id: &str, generation: u64, bytes: &[u8]) -> Result<Reply> {
        self.local_state_mut()?
            .external_ui_event(id, generation, bytes)
    }
    pub fn external_ui_close(&mut self, id: &str, generation: u64) -> Result<()> {
        self.local_state_mut()?.external_ui_close(id, generation)
    }
}
