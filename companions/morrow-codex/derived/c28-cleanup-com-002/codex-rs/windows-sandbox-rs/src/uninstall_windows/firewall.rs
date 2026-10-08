//! Removes known sandbox firewall rules while preserving unrelated rules.

use anyhow::Context;
use anyhow::Result;
use anyhow::anyhow;
use windows::Win32::NetworkManagement::WindowsFirewall::INetFwPolicy2;
use windows::Win32::NetworkManagement::WindowsFirewall::INetFwRules;
use windows::Win32::NetworkManagement::WindowsFirewall::NetFwPolicy2;
use windows::Win32::System::Com::CLSCTX_INPROC_SERVER;
use windows::Win32::System::Com::CoCreateInstance;
use windows::core::BSTR;

fn with_cleanup_firewall_rules(operation: impl FnOnce(&INetFwRules) -> Result<()>) -> Result<()> {
    crate::setup_provisioning::with_firewall_com_apartment(|| {
        let policy: INetFwPolicy2 = unsafe {
            CoCreateInstance(&NetFwPolicy2, /*punkouter*/ None, CLSCTX_INPROC_SERVER)
                .context("access firewall policy for sandbox uninstall")?
        };
        let rules = unsafe { policy.Rules() }.context("access sandbox firewall rules")?;
        operation(&rules)
    })
}

pub(super) fn cleanup_firewall_rules() -> Result<()> {
    with_cleanup_firewall_rules(|rules| {
        let mut errors = Vec::new();
        for name in [
            "codex_sandbox_offline_block_outbound",
            "codex_sandbox_offline_block_inbound",
            "codex_sandbox_offline_block_loopback_tcp",
            "codex_sandbox_offline_block_loopback_udp",
            "codex_sandbox_offline_allow_loopback_proxy",
        ] {
            if let Err(error) = unsafe { rules.Remove(&BSTR::from(name)) } {
                errors.push(format!("remove sandbox firewall rule {name}: {error}"));
            }
        }
        if errors.is_empty() {
            Ok(())
        } else {
            Err(anyhow!(errors.join("; ")))
        }
    })
}

#[cfg(test)]
#[path = "firewall_com_tests.rs"]
mod tests;
