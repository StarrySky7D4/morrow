use crate::{Result, Workbench, WorkbenchState, default_approval};
use morrow_plugin_runtime::inline_ui::{Failure, InlineUi, Reply};
use std::time::Duration;
pub struct PluginStatus {
    pub revision: u64,
    pub digest: Vec<u8>,
    pub enabled: bool,
    pub approved: bool,
    pub available: bool,
}
impl WorkbenchState {
    /// Explicit status refresh may retry bounded content maintenance after a failure.
    pub fn refresh_plugin_state(&mut self) -> Result<()> {
        if self.host.warning().is_some() {
            let _ = self.host.flush_pending();
        }
        if let Some(manager) = &self.manager
            && let Err(error) = self.pool.maintain(manager, &mut self.host)
        {
            self.plugin_warning = Some(format!("插件会话暂不可用，已有内容仍可读取。{error}"));
        }
        Ok(())
    }
    pub fn plugin_status(&self) -> PluginStatus {
        let selection = self.manager.as_ref().and_then(|m| {
            self.bundle
                .as_ref()
                .and_then(|b| m.selection(&b.manifest().package_id))
        });
        PluginStatus {
            revision: self.manager.as_ref().map_or(0, |m| m.revision()),
            digest: selection.map_or(vec![], |s| s.digest.to_vec()),
            enabled: selection.is_some_and(|s| s.enabled),
            approved: selection.is_some_and(|s| default_approval().is_subset(&s.approved)),
            available: self.bundle.is_some() && self.manager.is_some(),
        }
    }
    pub fn configure_plugin(&mut self, expected: u64, digest: &[u8], enable: bool) -> Result<()> {
        let bundle = self.bundle.as_ref().ok_or("工作台插件文件不可用。")?;
        let manager = self.manager.as_mut().ok_or("插件管理不可用。")?;
        if manager.revision() != expected || digest != bundle.digest() {
            return Err("插件状态已变化，请刷新后重新确认。".into());
        }
        if let Some(mut ui) = self.ui.take() {
            ui.close();
        }
        let id = &bundle.manifest().package_id;
        let result = if enable {
            let approved = default_approval()
                .intersection(bundle.capabilities())
                .copied()
                .collect();
            manager
                .approve(id, bundle.digest(), approved, manager.revision())
                .and_then(|_| manager.set_enabled(id, bundle.digest(), true, manager.revision()))
        } else {
            manager.set_enabled(id, bundle.digest(), false, manager.revision())
        };
        let closed = if let Some(session) = self.plugin.take() {
            self.pool.close(&mut self.host, &session)
        } else {
            Ok(())
        };
        let maintained = self.pool.maintain(manager, &mut self.host);
        result?;
        closed?;
        maintained?;
        if enable {
            let revision = manager.revision();
            self.plugin = Some(
                self.pool
                    .start(manager, &mut self.host, id, &[], revision)?,
            );
        }
        self.plugin_warning = None;
        Ok(())
    }
    pub fn ui_open(&mut self, seed: &str) -> Result<Reply> {
        if self.ui.is_some() {
            return Err("已有插件表单，请先关闭后重开。".into());
        }
        self.pool.maintain(
            self.manager.as_ref().ok_or("插件管理不可用。")?,
            &mut self.host,
        )?;
        let instance = self
            .pool
            .root(self.plugin.as_ref().ok_or("请先启用工作台插件。")?)?;
        self.ui_generation = self.ui_generation.checked_add(1).ok_or("界面代次耗尽")?;
        let mut ui = InlineUi::new(
            instance.package(),
            &self.host,
            instance.connection(),
            "workbench-tools",
            self.ui_generation,
            Duration::from_secs(3),
        )?;
        let reply = ui.open(
            instance.package(),
            &mut self.host,
            instance.connection(),
            seed,
        )?;
        self.ui = Some(ui);
        self.supervise_ui_reply(&reply);
        Ok(reply)
    }
    pub fn ui_event(&mut self, generation: u64, input: &[u8]) -> Result<Reply> {
        let ui = self.ui.as_mut().ok_or("插件表单已关闭。")?;
        if ui.generation() != generation {
            return Err("插件表单已更换，请重新打开。".into());
        }
        self.pool.maintain(
            self.manager.as_ref().ok_or("插件管理不可用。")?,
            &mut self.host,
        )?;
        let instance = self
            .pool
            .root(self.plugin.as_ref().ok_or("插件已停用。")?)?;
        match ui.event(
            instance.package(),
            &mut self.host,
            instance.connection(),
            input,
        ) {
            Ok(reply) => {
                self.supervise_ui_reply(&reply);
                Ok(reply)
            }
            Err(error) => {
                use morrow_plugin_runtime::inline_ui::Error;
                if matches!(error, Error::Core(_)) {
                    Ok(Reply {
                        view: ui.view().into(),
                        generation: ui.generation(),
                        revision: ui.revision(),
                        serial: ui.serial(),
                        document: None,
                        failure: Some(Failure::InvalidDocument),
                    })
                } else {
                    Err(error.into())
                }
            }
        }
    }
    // Inline UI uses the same root connection. Execution faults must stop that root,
    // while preserving the original UI failure reply and never retrying the event.
    fn supervise_ui_reply(&mut self, reply: &Reply) {
        use morrow_plugin_runtime::Fault;
        if !matches!(
            &reply.failure,
            Some(Failure::Execution(
                Fault::Trap | Fault::TaskProtocol | Fault::Limits
            ))
        ) {
            return;
        }
        if let Some(session) = &self.plugin
            && let Ok(instance) = self.pool.root(session)
        {
            instance.stop();
        }
        self.finish_stopped_session();
        if let Some(manager) = &self.manager
            && let Err(error) = self.pool.maintain(manager, &mut self.host)
        {
            self.plugin_warning = Some(format!("插件会话已停止，清理暂未完成。{error}"));
        }
    }
    pub fn ui_close(&mut self, generation: u64) {
        if self
            .ui
            .as_ref()
            .is_some_and(|s| s.generation() == generation)
            && let Some(mut ui) = self.ui.take()
        {
            ui.close();
        }
    }
}

impl Workbench {
    pub fn refresh_plugin_state(&mut self) -> Result<()> {
        self.local_state_mut()?.refresh_plugin_state()
    }
    pub fn plugin_status(&self) -> Result<PluginStatus> {
        Ok(self.local_state()?.plugin_status())
    }
    pub fn configure_plugin(&mut self, expected: u64, digest: &[u8], enable: bool) -> Result<()> {
        self.local_state_mut()?
            .configure_plugin(expected, digest, enable)
    }
    pub fn ui_open(&mut self, seed: &str) -> Result<Reply> {
        self.local_state_mut()?.ui_open(seed)
    }
    pub fn ui_event(&mut self, generation: u64, input: &[u8]) -> Result<Reply> {
        self.local_state_mut()?.ui_event(generation, input)
    }
    pub fn ui_close(&mut self, generation: u64) -> Result<()> {
        self.local_state_mut()?.ui_close(generation);
        Ok(())
    }
}
