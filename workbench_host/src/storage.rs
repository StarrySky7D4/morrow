//! Windows owns the protected key and lease for the whole host lifetime.
//! Maintenance runs before a new write, never reclassifies a committed result.
use crate::Result;
#[cfg(target_os = "windows")]
use morrow_audit::session::{OpenMode, Session, SessionError};
use morrow_core::dispatch::HostRuntime;
use std::{
    ops::{Deref, DerefMut},
    path::Path,
};

pub struct Storage {
    #[cfg(target_os = "windows")]
    session: Session,
    #[cfg(not(target_os = "windows"))]
    host: HostRuntime,
    warning: Option<String>,
    #[cfg(target_os = "windows")]
    _registry: Option<morrow_audit::library::Registry>,
}
impl Storage {
    #[cfg(target_os = "windows")]
    pub fn open(path: &Path) -> Result<Self> {
        let session = Session::open(path, Default::default(), OpenMode::Initialize)
            .map_err(session_message)?;
        let mut storage = Self {
            session,
            warning: None,
            _registry: None,
        };
        // A maintenance failure must not hide already durable content.
        let _ = storage.flush_pending();
        Ok(storage)
    }
    #[cfg(not(target_os = "windows"))]
    pub fn open(_path: &Path) -> Result<Self> {
        Err("此平台的内容库密钥保护后端尚未接入。".into())
    }
    #[cfg(target_os = "windows")]
    pub fn open_managed(root: &Path) -> Result<Self> {
        let mut registry = morrow_audit::library::Registry::open(root).map_err(library_message)?;
        let session = registry
            .open_session(Default::default())
            .map_err(library_message)?;
        let mut storage = Self {
            session,
            warning: None,
            _registry: Some(registry),
        };
        let _ = storage.flush_pending();
        Ok(storage)
    }
    #[cfg(not(target_os = "windows"))]
    pub fn open_managed(_root: &Path) -> Result<Self> {
        Err("此平台的活动内容库管理尚未接入。".into())
    }
    pub fn backup_snapshot(&self, destination: &Path) -> Result<()> {
        #[cfg(target_os = "windows")]
        {
            self.session
                .backup_snapshot(destination)
                .map_err(session_message)?;
            Ok(())
        }
        #[cfg(not(target_os = "windows"))]
        {
            let _ = destination;
            Err("此平台的内容库快照后端尚未接入。".into())
        }
    }
    pub fn backup_key(&self, destination: &Path) -> Result<()> {
        #[cfg(target_os = "windows")]
        {
            self.session.backup_key(destination).map_err(|e| match e {
                SessionError::Io(_) => "备份未完成，请检查保存位置是否可写。",
                other => session_message(other),
            })?;
            Ok(())
        }
        #[cfg(not(target_os = "windows"))]
        {
            let _ = destination;
            Err("此平台的内容库密钥保护后端尚未接入。".into())
        }
    }
    pub fn warning(&self) -> Option<&str> {
        self.warning.as_deref()
    }
    pub fn prepare_write(&mut self) -> Result<()> {
        let (events, bytes) = self.store_local().pending_usage()?;
        if self.warning.is_some() || events >= 64 || bytes >= 8 * 1024 * 1024 {
            self.flush_pending()?;
        }
        Ok(())
    }
    pub fn flush_pending(&mut self) -> Result<()> {
        let result = self.flush_impl();
        if result.is_err() {
            let message = "内容库维护暂未完成，已有内容仍可读取；请检查磁盘空间后重试保存。";
            self.warning = Some(message.into());
            return Err(message.into());
        }
        self.warning = None;
        Ok(())
    }
    #[cfg(target_os = "windows")]
    fn flush_impl(&mut self) -> Result<()> {
        #[cfg(feature = "fault-injection")]
        if std::env::var("MORROW_WORKBENCH_FAIL_SEAL").as_deref() == Ok("1") {
            return Err("injected seal failure".into());
        }
        if self.session.flush(16)?.more_pending {
            return Err("bounded maintenance has more pending work".into());
        }
        Ok(())
    }
    #[cfg(not(target_os = "windows"))]
    fn flush_impl(&mut self) -> Result<()> {
        Err("Protected storage unavailable".into())
    }
}
impl Deref for Storage {
    type Target = HostRuntime;
    fn deref(&self) -> &Self::Target {
        #[cfg(target_os = "windows")]
        {
            self.session.runtime_ref()
        }
        #[cfg(not(target_os = "windows"))]
        {
            &self.host
        }
    }
}
impl DerefMut for Storage {
    fn deref_mut(&mut self) -> &mut Self::Target {
        #[cfg(target_os = "windows")]
        {
            self.session.runtime()
        }
        #[cfg(not(target_os = "windows"))]
        {
            &mut self.host
        }
    }
}

#[cfg(target_os = "windows")]
pub(crate) fn session_message(e: SessionError) -> &'static str {
    match e {
        SessionError::MissingKey => "内容库保护密钥缺失，请恢复原 .audit-key 文件后重试。",
        SessionError::KeyMismatch | SessionError::Key(_) => {
            "内容库保护密钥不匹配或无法解密，请使用原文件及原系统账户。"
        }
        SessionError::KeyWithoutDatabase => "保护密钥仍在，但内容库缺失或为空，请恢复原内容库。",
        SessionError::IdentityBusy => {
            "同一内容库身份的另一份副本正在使用中，请先关闭原工作台再打开此副本。"
        }
        SessionError::Busy => "此内容库正在由另一个进程使用，请关闭另一个窗口后重试。",
        SessionError::SnapshotDestinationExists => "恢复目标已存在，请选择尚不存在的新目录。",
        SessionError::SnapshotPublishUnknown => {
            "内容库恢复结果需要核对，请检查目标目录；原内容库未被替换。"
        }
        SessionError::SnapshotFormat => "内容库备份格式或校验不正确，请保留原备份文件。",
        SessionError::BackupAlreadyExists => "备份位置已有文件，请选择新的文件名。",
        SessionError::BackupPublishUnknown => "备份结果需要核对，请保留当前文件并检查保存位置。",
        SessionError::RecoveryRequiresBinding => {
            "此内容库尚未绑定保护文件，无法核对所选文件的归属。"
        }
        SessionError::RecoveryPublishUnknown => {
            "恢复结果需要核对，请重试打开；原保护文件副本已保留（若此前存在）。"
        }
        _ => "内容库无法验证或打开，请保留原内容库与保护密钥后重试。",
    }
}

#[cfg(target_os = "windows")]
pub(crate) fn library_message(e: morrow_audit::library::Error) -> &'static str {
    use morrow_audit::library::Error;
    match e {
        Error::Busy => "工作台仍在运行，请先关闭后再切换内容库。",
        Error::IdentityMismatch => "已登记的内容库身份不匹配，请保留原资料并选择正确备份恢复。",
        Error::Invalid => "活动内容库登记已损坏或不受支持，已停止打开以保护资料。",
        Error::PublishUnknown => "内容库切换结果尚未确认，请重新打开工作台核对。",
        Error::Io(_) => "活动内容库或登记文件无法读取，请检查原位置；不会自动创建替代内容库。",
        Error::Session(e) => session_message(e),
    }
}
