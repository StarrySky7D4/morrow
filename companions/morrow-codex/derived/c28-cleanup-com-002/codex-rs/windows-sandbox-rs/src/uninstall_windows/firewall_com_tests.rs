//! Read-only COM regression: never add, change, or remove a firewall rule.

use super::with_cleanup_firewall_rules;

#[test]
fn cleanup_firewall_collection_opens_on_fresh_thread() {
    std::thread::spawn(|| {
        // A fresh thread has no inherited COM apartment. Use the production access path.
        with_cleanup_firewall_rules(|_| Ok(())).expect("read firewall collection on fresh thread");
        // Open again after the first scope, exercising teardown and reinitialization.
        with_cleanup_firewall_rules(|_| Ok(())).expect("read firewall collection after scope ended");
    })
    .join()
    .expect("fresh firewall thread");
}

#[test]
fn cleanup_firewall_collection_callback_error_keeps_later_access_available() {
    std::thread::spawn(|| {
        let error = with_cleanup_firewall_rules(|_| -> anyhow::Result<()> {
            Err(anyhow::anyhow!("synthetic read-only callback failure"))
        })
        .expect_err("callback failure must propagate");
        assert_eq!(error.to_string(), "synthetic read-only callback failure");
        with_cleanup_firewall_rules(|_| Ok(()))
            .expect("read firewall collection after failed callback");
    })
    .join()
    .expect("fallible firewall thread");
}

#[test]
fn owned_firewall_verification_rejects_foreign_sid_and_altered_properties() {
    use super::ObservedFirewallRule;
    use super::verify_owned_firewall_rules_plan;
    use crate::setup_provisioning::OwnedFirewallProof;
    use crate::setup_provisioning::OwnedFirewallRuleProof;

    let sid = "S-1-5-21-10-20-30-1001";
    let user_spec = format!("O:LSD:(A;;CC;;;{sid})");
    let proof = OwnedFirewallProof {
        offline_sid: sid.to_string(),
        rules: vec![OwnedFirewallRuleProof {
            internal_name: "codex_sandbox_offline_block_loopback_tcp",
            friendly_desc: "Codex Sandbox Offline - Block Loopback TCP".to_string(),
            direction: 2,
            action: 0,
            enabled: true,
            profiles: 0x7fffffff,
            protocol: 6,
            local_user_authorized_list: user_spec.clone(),
            offline_sid: sid.to_string(),
            remote_addresses: "127.0.0.1/255.0.0.0,::1/128".to_string(),
            remote_ports: None,
        }],
    };
    let observed = vec![ObservedFirewallRule {
        internal_name: "codex_sandbox_offline_block_loopback_tcp".to_string(),
        friendly_desc: "Codex Sandbox Offline - Block Loopback TCP".to_string(),
        direction: 2,
        action: 0,
        enabled: true,
        profiles: 0x7fffffff,
        protocol: 6,
        local_user_authorized_list: user_spec,
        remote_addresses: "127.0.0.1/255.0.0.0,::1/128".to_string(),
        remote_ports: None,
    }];

    assert!(verify_owned_firewall_rules_plan(&proof, sid, &observed).is_ok());
    assert!(
        verify_owned_firewall_rules_plan(&proof, "S-1-5-21-10-20-30-9999", &observed).is_err(),
        "mismatched expected principal SID must fail"
    );

    let mut foreign_sid_observed = observed.clone();
    foreign_sid_observed[0].local_user_authorized_list =
        "O:LSD:(A;;CC;;;S-1-5-21-10-20-30-9999)".to_string();
    assert!(
        verify_owned_firewall_rules_plan(&proof, sid, &foreign_sid_observed).is_err(),
        "foreign rule LocalUserAuthorizedList must fail"
    );

    let mut disabled_observed = observed.clone();
    disabled_observed[0].enabled = false;
    assert!(
        verify_owned_firewall_rules_plan(&proof, sid, &disabled_observed).is_err(),
        "disabled rule must fail"
    );

    let mut uncreated_legacy_proof = proof;
    uncreated_legacy_proof.rules[0].internal_name = "codex_sandbox_offline_allow_loopback_proxy";
    let mut uncreated_legacy_observed = observed;
    uncreated_legacy_observed[0].internal_name =
        "codex_sandbox_offline_allow_loopback_proxy".to_string();
    assert!(
        verify_owned_firewall_rules_plan(
            &uncreated_legacy_proof,
            sid,
            &uncreated_legacy_observed
        )
        .is_err(),
        "uncreated legacy rule must never be accepted as owned"
    );
}

