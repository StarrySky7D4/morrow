//! Removes known sandbox firewall rules while preserving unrelated rules.

use anyhow::Context;
use anyhow::Result;
use anyhow::anyhow;
use anyhow::bail;
use anyhow::ensure;
use windows::Win32::Foundation::VARIANT_TRUE;
use windows::Win32::NetworkManagement::WindowsFirewall::INetFwPolicy2;
use windows::Win32::NetworkManagement::WindowsFirewall::INetFwRule3;
use windows::Win32::NetworkManagement::WindowsFirewall::INetFwRules;
use windows::Win32::NetworkManagement::WindowsFirewall::NetFwPolicy2;
use windows::Win32::System::Com::CLSCTX_INPROC_SERVER;
use windows::Win32::System::Com::CoCreateInstance;
use windows::core::BSTR;
use windows::core::Interface;

use crate::setup_provisioning::OwnedFirewallProof;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ObservedFirewallRule {
    pub internal_name: String,
    pub friendly_desc: String,
    pub direction: i32,
    pub action: i32,
    pub enabled: bool,
    pub profiles: i32,
    pub protocol: i32,
    pub local_user_authorized_list: String,
    pub remote_addresses: String,
    pub remote_ports: Option<String>,
}

pub(crate) fn verify_owned_firewall_rules_plan(
    proof: &OwnedFirewallProof,
    expected_offline_sid: &str,
    observed: &[ObservedFirewallRule],
) -> Result<()> {
    ensure!(
        proof.offline_sid == expected_offline_sid,
        "owned firewall proof offline SID does not match owned principal SID: proof={}, expected={expected_offline_sid}",
        proof.offline_sid
    );
    ensure!(
        !proof.rules.is_empty(),
        "owned firewall proof must contain at least one rule"
    );
    ensure!(
        observed.len() == proof.rules.len(),
        "owned firewall rule observation count mismatch: expected {}, got {}",
        proof.rules.len(),
        observed.len()
    );
    for expected in &proof.rules {
        ensure!(
            expected.internal_name != "codex_sandbox_offline_allow_loopback_proxy",
            "uncreated legacy firewall rule {} cannot be retired by normal owned cleanup",
            expected.internal_name
        );
        ensure!(
            expected.offline_sid == proof.offline_sid
                && expected
                    .local_user_authorized_list
                    .contains(&proof.offline_sid),
            "owned firewall proof rule {} has unexpected LocalUserAuthorizedList or SID",
            expected.internal_name
        );
        let candidate = observed
            .iter()
            .find(|obs| obs.internal_name == expected.internal_name)
            .ok_or_else(|| {
                anyhow!(
                    "owned firewall rule missing from observation: {}",
                    expected.internal_name
                )
            })?;
        ensure!(
            candidate.friendly_desc == expected.friendly_desc,
            "owned firewall rule {} Description changed: expected {}, got {}",
            expected.internal_name,
            expected.friendly_desc,
            candidate.friendly_desc
        );
        ensure!(
            candidate.direction == expected.direction,
            "owned firewall rule {} Direction changed: expected {}, got {}",
            expected.internal_name,
            expected.direction,
            candidate.direction
        );
        ensure!(
            candidate.action == expected.action,
            "owned firewall rule {} Action changed: expected {}, got {}",
            expected.internal_name,
            expected.action,
            candidate.action
        );
        ensure!(
            candidate.enabled == expected.enabled,
            "owned firewall rule {} Enabled changed: expected {}, got {}",
            expected.internal_name,
            expected.enabled,
            candidate.enabled
        );
        ensure!(
            candidate.profiles == expected.profiles,
            "owned firewall rule {} Profiles changed: expected {}, got {}",
            expected.internal_name,
            expected.profiles,
            candidate.profiles
        );
        ensure!(
            candidate.protocol == expected.protocol,
            "owned firewall rule {} Protocol changed: expected {}, got {}",
            expected.internal_name,
            expected.protocol,
            candidate.protocol
        );
        ensure!(
            candidate.local_user_authorized_list == expected.local_user_authorized_list
                && candidate
                    .local_user_authorized_list
                    .contains(expected_offline_sid),
            "owned firewall rule {} LocalUserAuthorizedList changed or belongs to another SID",
            expected.internal_name
        );
        ensure!(
            candidate.remote_addresses == expected.remote_addresses,
            "owned firewall rule {} RemoteAddresses changed: expected {}, got {}",
            expected.internal_name,
            expected.remote_addresses,
            candidate.remote_addresses
        );
        if let Some(expected_ports) = &expected.remote_ports {
            ensure!(
                candidate.remote_ports.as_deref() == Some(expected_ports.as_str()),
                "owned firewall rule {} RemotePorts changed: expected {expected_ports}, got {:?}",
                expected.internal_name,
                candidate.remote_ports
            );
        }
    }
    Ok(())
}

fn with_cleanup_firewall_rules<T>(
    operation: impl FnOnce(&INetFwRules) -> Result<T>,
) -> Result<T> {
    crate::setup_provisioning::with_firewall_com_apartment(|| {
        let policy: INetFwPolicy2 = unsafe {
            CoCreateInstance(&NetFwPolicy2, /*punkouter*/ None, CLSCTX_INPROC_SERVER)
                .context("access firewall policy for sandbox uninstall")?
        };
        let rules = unsafe { policy.Rules() }.context("access sandbox firewall rules")?;
        operation(&rules)
    })
}

fn observe_owned_firewall_rules_in_collection(
    rules: &INetFwRules,
    proof: &OwnedFirewallProof,
) -> Result<Vec<ObservedFirewallRule>> {
    let mut observed = Vec::with_capacity(proof.rules.len());
    for expected in &proof.rules {
        let raw_rule = unsafe { rules.Item(&BSTR::from(expected.internal_name)) }.with_context(
            || {
                format!(
                    "owned firewall rule missing before retirement: {}",
                    expected.internal_name
                )
            },
        )?;
        let rule: INetFwRule3 = raw_rule.cast().with_context(|| {
            format!(
                "owned firewall rule INetFwRule3 cast failed before retirement: {}",
                expected.internal_name
            )
        })?;
        let friendly_desc = unsafe { rule.Description() }
            .with_context(|| {
                format!(
                    "read Description for firewall rule {}",
                    expected.internal_name
                )
            })?
            .to_string();
        let direction = unsafe { rule.Direction() }
            .with_context(|| format!("read Direction for firewall rule {}", expected.internal_name))?
            .0;
        let action = unsafe { rule.Action() }
            .with_context(|| format!("read Action for firewall rule {}", expected.internal_name))?
            .0;
        let enabled = unsafe { rule.Enabled() }
            .with_context(|| format!("read Enabled for firewall rule {}", expected.internal_name))?
            == VARIANT_TRUE;
        let profiles = unsafe { rule.Profiles() }
            .with_context(|| format!("read Profiles for firewall rule {}", expected.internal_name))?;
        let protocol = unsafe { rule.Protocol() }
            .with_context(|| format!("read Protocol for firewall rule {}", expected.internal_name))?;
        let local_user_authorized_list = unsafe { rule.LocalUserAuthorizedList() }
            .with_context(|| {
                format!(
                    "read LocalUserAuthorizedList for firewall rule {}",
                    expected.internal_name
                )
            })?
            .to_string();
        let remote_addresses = unsafe { rule.RemoteAddresses() }
            .with_context(|| {
                format!(
                    "read RemoteAddresses for firewall rule {}",
                    expected.internal_name
                )
            })?
            .to_string();
        let remote_ports = if expected.remote_ports.is_some() {
            Some(
                unsafe { rule.RemotePorts() }
                    .with_context(|| {
                        format!(
                            "read RemotePorts for firewall rule {}",
                            expected.internal_name
                        )
                    })?
                    .to_string(),
            )
        } else {
            None
        };
        observed.push(ObservedFirewallRule {
            internal_name: expected.internal_name.to_string(),
            friendly_desc,
            direction,
            action,
            enabled,
            profiles,
            protocol,
            local_user_authorized_list,
            remote_addresses,
            remote_ports,
        });
    }
    Ok(observed)
}

pub(super) fn verify_owned_firewall_rules(
    proof: &OwnedFirewallProof,
    expected_offline_sid: &str,
) -> Result<()> {
    with_cleanup_firewall_rules(|rules| {
        let observed = observe_owned_firewall_rules_in_collection(rules, proof)?;
        verify_owned_firewall_rules_plan(proof, expected_offline_sid, &observed)
    })
}

pub(super) fn remove_owned_firewall_rules(
    proof: &OwnedFirewallProof,
    expected_offline_sid: &str,
) -> Result<Vec<&'static str>> {
    with_cleanup_firewall_rules(|rules| {
        let observed = observe_owned_firewall_rules_in_collection(rules, proof)?;
        verify_owned_firewall_rules_plan(proof, expected_offline_sid, &observed)?;
        let mut removed = Vec::with_capacity(proof.rules.len());
        for expected in &proof.rules {
            if let Err(error) = unsafe { rules.Remove(&BSTR::from(expected.internal_name)) } {
                bail!(
                    "remove owned sandbox firewall rule {}: {error}",
                    expected.internal_name
                );
            }
            removed.push(expected.internal_name);
        }
        Ok(removed)
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

