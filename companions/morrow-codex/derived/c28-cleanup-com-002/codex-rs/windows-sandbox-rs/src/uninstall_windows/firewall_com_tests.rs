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
        let error = with_cleanup_firewall_rules(|_| {
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
