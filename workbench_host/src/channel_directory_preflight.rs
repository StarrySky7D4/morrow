//! Capacity checks use an inert canonical Directory before creating a live grant.
//! The representative identities are never returned, persisted or bound.
use crate::Result;
use morrow_core::channel::{Budget, Directory, Endpoint, Kind};

pub(crate) const DIRECTORY_INPUT_TYPE: &str = "morrow.channel.directory.v1";

pub(crate) fn preflight_directory_input(
    input_type: &str,
    max_input_bytes: u32,
    kind: Kind,
    effective_budget: Budget,
) -> Result<()> {
    if input_type != DIRECTORY_INPUT_TYPE {
        // Explicit private input formats, including the legacy 65-byte route,
        // retain their own admission and run checks.
        return Ok(());
    }
    let bytes = Directory {
        scope_sha256: [0xd1; 32],
        channels: vec![Endpoint {
            reference: [0xd2; 32],
            source_epoch: [0xd3; 32],
            kind,
            budget: effective_budget,
        }],
    }
    .encode()?;
    if bytes.len() > max_input_bytes as usize {
        return Err("handler cannot accept the canonical local channel Directory".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use sha2::{Digest, Sha256};

    fn minimum() -> Budget {
        Budget {
            max_channels: 1,
            max_frame_bytes: 1,
            max_bytes: 1,
            max_messages: 1,
            max_requests: 1,
            max_duration_ms: 1,
        }
    }
    fn canonical(kind: Kind, budget: Budget) -> Vec<u8> {
        Directory {
            scope_sha256: [4; 32],
            channels: vec![Endpoint {
                reference: [2; 32],
                source_epoch: [3; 32],
                kind,
                budget,
            }],
        }
        .encode()
        .unwrap()
    }
    #[test]
    fn both_kinds_minimum_and_maximum_are_exactly_256_bytes() {
        for kind in [Kind::ByteStream, Kind::Events] {
            for budget in [minimum(), Budget::default()] {
                let bytes = canonical(kind, budget);
                assert_eq!(bytes.len(), 256);
                assert_eq!(&bytes[..8], &[0, 0, 0, 0, 31, 0, 0, 0]);
                assert_eq!(
                    Directory::decode(&bytes).unwrap().channels[0].budget,
                    budget
                );
                assert!(
                    preflight_directory_input(DIRECTORY_INPUT_TYPE, 255, kind, budget).is_err()
                );
                assert!(preflight_directory_input(DIRECTORY_INPUT_TYPE, 256, kind, budget).is_ok());
            }
        }
    }
    #[test]
    fn canonical_contract_goldens_remain_unchanged() {
        for (kind, budget, expected) in [
            (
                Kind::ByteStream,
                minimum(),
                "19d64769c0b2b1025c795052a77c46473034c6b426e9383cdae6a0bd735b85ab",
            ),
            (
                Kind::ByteStream,
                Budget::default(),
                "fa9e07f5294e5100fb14cedaf6d04ac89eb4b690d316ea0e90c681684ffe8c1f",
            ),
            (
                Kind::Events,
                minimum(),
                "a6bf88b30038f0d199a295b8c814f8f27663e520c0fc752d97433e7b5c4e2f7a",
            ),
            (
                Kind::Events,
                Budget::default(),
                "b542b0aa88759b678a5bacd68ec782bf13aee3fd9b430ff8244f7301f3dc6cac",
            ),
        ] {
            let actual: String = Sha256::digest(canonical(kind, budget))
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect();
            assert_eq!(actual, expected);
        }
    }
    #[test]
    fn private_legacy_capacity_is_not_reinterpreted_as_directory() {
        assert!(preflight_directory_input("bytes", 65, Kind::Events, minimum()).is_ok());
        assert!(
            preflight_directory_input("private.channel.exercise", 65, Kind::ByteStream, minimum())
                .is_ok()
        );
    }
    #[test]
    fn typed_directory_rejects_invalid_budget_without_live_identity() {
        let mut invalid = minimum();
        invalid.max_requests = 0;
        assert!(
            preflight_directory_input(DIRECTORY_INPUT_TYPE, 65536, Kind::Events, invalid).is_err()
        );
    }
}
