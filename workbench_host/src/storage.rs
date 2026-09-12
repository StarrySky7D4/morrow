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
}
impl Storage {
    #[cfg(target_os = "windows")]
    pub fn open(path: &Path) -> Result<Self> {
        let session =
            Session::open(path, Default::default(), OpenMode::Initialize).map_err(|e| match e {
                SessionError::MissingKey => "内容库保护密钥缺失，请恢复原 .audit-key 文件后重试。",
                SessionError::KeyMismatch | SessionError::Key(_) => {
                    "内容库保护密钥不匹配或无法解密，请使用原文件及原系统账户。"
                }
                SessionError::KeyWithoutDatabase => {
                    "保护密钥仍在，但内容库缺失或为空，请恢复原内容库。"
                }
                SessionError::Busy => "此内容库正在由另一个进程使用，请关闭另一个窗口后重试。",
                _ => "内容库无法验证或打开，请保留原内容库与保护密钥后重试。",
            })?;
        let mut storage = Self {
            session,
            warning: None,
        };
        // A maintenance failure must not hide already durable content.
        let _ = storage.flush_pending();
        Ok(storage)
    }
    #[cfg(not(target_os = "windows"))]
    pub fn open(_path: &Path) -> Result<Self> {
        Err("此平台的内容库密钥保护后端尚未接入。".into())
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
