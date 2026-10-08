//! Actual OS inventory matched to an operator-reviewed guest UUID, not a bool option.
//! This is not hardware attestation; require a clean dedicated VM under exclusive management.
use crate::preflight::{Artifact, observe_artifact};
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use std::process::Command;

pub const SCRIPT: &str = include_str!("../guest_identity.ps1");
pub const CONFIRMATION: &str = "READ_ACTUAL_LOCAL_HYPERV_GUEST_IDENTITY_AND_CREATE_FRESH_EVIDENCE";
#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct GuestIdentity {
    schema: String,
    manufacturer: String,
    model: String,
    pub firmware_uuid: String,
    machine_name: String,
    hypervisor_present: bool,
    administrator: bool,
    process_64_bit: bool,
    os_64_bit: bool,
    observed_utc: String,
}
pub struct Evidence {
    pub identity: GuestIdentity,
}
pub fn valid_uuid(uuid: &str) -> bool {
    uuid.len() == 36
        && uuid.bytes().enumerate().all(|(i, b)| {
            if [8, 13, 18, 23].contains(&i) {
                b == b'-'
            } else {
                b.is_ascii_digit() || (b'a'..=b'f').contains(&b)
            }
        })
        && uuid != "00000000-0000-0000-0000-000000000000"
        && uuid != "ffffffff-ffff-ffff-ffff-ffffffffffff"
}
impl GuestIdentity {
    pub fn validate(&self, expected_uuid: &str) -> Result<()> {
        ensure!(
            valid_uuid(expected_uuid) && self.firmware_uuid == expected_uuid,
            "actual guest UUID mismatch; no setup"
        );
        ensure!(
            self.schema == "morrow-dedicated-hyperv-guest-identity-v1"
                && self.manufacturer == "Microsoft Corporation"
                && self.model == "Virtual Machine"
                && self.hypervisor_present
                && self.administrator
                && self.process_64_bit
                && self.os_64_bit,
            "actual local inventory is not the reviewed elevated Hyper-V guest; no physical-host fallback"
        );
        ensure!(
            !self.machine_name.is_empty()
                && self.machine_name.len() <= 64
                && self.observed_utc.len() <= 64,
            "guest inventory bounds"
        );
        Ok(())
    }
}
pub fn observe(
    shell: &Artifact,
    expected_uuid: &str,
    journal: &mut crate::evidence::Journal,
) -> Result<Evidence> {
    observe_artifact(shell)?;
    ensure!(
        matches!(
            shell.path.file_name().and_then(|p| p.to_str()),
            Some("powershell.exe" | "pwsh.exe")
        ),
        "only the explicitly pinned PowerShell image"
    );
    let output = Command::new(&shell.path)
        .args([
            "-NoLogo",
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            SCRIPT,
        ])
        .output()?;
    journal.raw("guest-identity-stdout.log", &output.stdout)?;
    journal.raw("guest-identity-stderr.log", &output.stderr)?;
    journal.record(
        "guest-identity-actual-exit",
        &serde_json::json!({"exit_code":output.status.code(), "success":output.status.success()}),
    )?;
    ensure!(
        output.stdout.len() <= 32 * 1024 && output.stderr.len() <= 32 * 1024,
        "guest probe output bounds"
    );
    ensure!(
        output.status.success(),
        "guest identity probe failed; no fallback"
    );
    let identity: GuestIdentity = serde_json::from_slice(&output.stdout)?;
    identity.validate(expected_uuid)?;
    Ok(Evidence { identity })
}

#[cfg(test)]
mod tests {
    use super::*;
    const UUID: &str = "01234567-89ab-cdef-0123-456789abcdef";
    fn synthetic_inventory() -> GuestIdentity {
        // Parser/preflight policy only. These literals are never supplied to a
        // production OS observation or treated as an execution permission.
        serde_json::from_value(serde_json::json!({
            "schema":"morrow-dedicated-hyperv-guest-identity-v1",
            "manufacturer":"Microsoft Corporation", "model":"Virtual Machine",
            "firmware_uuid":UUID, "machine_name":"SYNTHETIC",
            "hypervisor_present":true, "administrator":true,
            "process_64_bit":true, "os_64_bit":true, "observed_utc":"synthetic-parser-test"
        }))
        .unwrap()
    }
    #[test]
    fn physical_hyperv_host_is_rejected_even_with_hypervisor_present() {
        let mut value = synthetic_inventory();
        value.manufacturer = "Physical Host Vendor".into();
        value.model = "Physical Workstation".into();
        assert!(value.validate(UUID).is_err());
    }
    #[test]
    fn approved_uuid_does_not_grant_local_admin_or_override_observed_uuid() {
        let mut value = synthetic_inventory();
        value.administrator = false;
        assert!(value.validate(UUID).is_err());
        value.administrator = true;
        assert!(
            value
                .validate("01234567-89ab-cdef-0123-456789abcdee")
                .is_err()
        );
    }
    #[test]
    fn malformed_identity_and_unknown_probe_fields_cannot_grant_a_guest() {
        assert!(!valid_uuid("00000000-0000-0000-0000-000000000000"));
        assert!(!valid_uuid("01234567-89AB-CDEF-0123-456789ABCDEF"));
        let mut raw = serde_json::to_value(synthetic_inventory()).unwrap();
        raw["permission_granted"] = serde_json::json!(true);
        assert!(serde_json::from_value::<GuestIdentity>(raw).is_err());
    }
}
