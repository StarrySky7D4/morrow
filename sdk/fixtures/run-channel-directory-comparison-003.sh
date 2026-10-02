#!/usr/bin/env bash
# Only this one fresh baseline-contract comparison pair.
set -euo pipefail
repo="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$repo"
for mode in reusable oneshot; do
    project="sdk/fixtures/rust-channel-directory-${mode}-003"
    while read -r digest relative; do
        case "$relative" in
            build/plugin.wasm) suffix=WASM ;;
            dist/*.mplugin) suffix=PACKAGE ;;
            *) echo "Unexpected artifact pin: $relative" >&2; exit 1 ;;
        esac
        prefix="MORROW_DIRECTORY_COMPARISON_${mode^^}"
        export "${prefix}_${suffix}=$repo/$project/$relative"
        export "${prefix}_${suffix}_SHA256=$digest"
    done < "$project/ARTIFACT_SHA256SUMS"
    (cd "$project" && sha256sum --check ARTIFACT_SHA256SUMS)
done
cargo test --locked --offline --manifest-path plugin_runtime/Cargo.toml --features packages \
    --test channel_directory_comparison -- --ignored --nocapture --test-threads=1
