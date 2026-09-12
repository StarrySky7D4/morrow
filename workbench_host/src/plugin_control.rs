use crate::{Result, Workbench, default_approval};
use morrow_plugin_runtime::inline_ui::{Failure, InlineUi, Reply};
use std::time::Duration;
pub struct PluginStatus {
    pub revision: u64,
    pub digest: Vec<u8>,
    pub enabled: bool,
    pub approved: bool,
    pub available: bool,
}
impl Workbench {
    /// Explicit status refresh may retry bounded content maintenance after a failure.
    pub fn refresh_plugin_state(&mut self) {
        if self.host.warning().is_some() {
            let _ = self.host.flush_pending();
        }
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
        if let Some(previous) = self.plugin.take() {
            let _ = previous.close(&mut self.host);
        }
        result?;
        if enable {
            self.plugin = Some(manager.connect(id, &mut self.host)?);
        }
        self.plugin_warning = None;
        Ok(())
    }
    pub fn ui_open(&mut self, seed: &str) -> Result<Reply> {
        if self.ui.is_some() {
            return Err("已有插件表单，请先关闭后重开。".into());
        }
        let instance = self.plugin.as_ref().ok_or("请先启用工作台插件。")?;
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
        Ok(reply)
    }
    pub fn ui_event(&mut self, generation: u64, input: &[u8]) -> Result<Reply> {
        let ui = self.ui.as_mut().ok_or("插件表单已关闭。")?;
        if ui.generation() != generation {
            return Err("插件表单已更换，请重新打开。".into());
        }
        let instance = self.plugin.as_ref().ok_or("插件已停用。")?;
        match ui.event(
            instance.package(),
            &mut self.host,
            instance.connection(),
            input,
        ) {
            Ok(reply) => Ok(reply),
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
