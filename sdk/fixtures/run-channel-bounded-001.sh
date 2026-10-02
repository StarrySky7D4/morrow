#!/usr/bin/env bash
# Run explicitly ignored tests against the four new, independently pinned guests.
set -euo pipefail
repo="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$repo"
for mode in reusable oneshot standard-reusable standard-oneshot; do
    case "$mode" in
        reusable|oneshot) project="sdk/fixtures/rust-channel-bounded-${mode}-001" ;;
        standard-*) project="sdk/fixtures/rust-channel-${mode}-002" ;;
    esac
    while read -r digest relative; do
        case "$relative" in
            build/plugin.wasm) suffix=WASM ;;
            dist/*.mplugin) suffix=PACKAGE ;;
            *) echo "Unexpected artifact pin: $relative" >&2; exit 1 ;;
        esac
        prefix="MORROW_BOUNDED_${mode^^}"
        prefix="${prefix//-/_}"
        export "${prefix}_${suffix}=$repo/$project/$relative"
        export "${prefix}_${suffix}_SHA256=$digest"
    done < "$project/ARTIFACT_SHA256SUMS"
    (cd "$project" && sha256sum --check ARTIFACT_SHA256SUMS)
done
cargo test --locked --offline --manifest-path plugin_runtime/Cargo.toml --features packages \
    --test channel_bounded_sdk -- --ignored --nocapture --test-threads=1
