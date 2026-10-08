# Portable Windows qualification helpers

The four public component helpers require explicit local configuration:

- `--rust-bin RUST_BIN`: directory containing cargo.exe, rustc.exe and rustdoc.exe.
- `--vs-dev-cmd VS_DEV_CMD`: reviewed Visual Studio VsDevCmd.bat path.
- `--capnp-bin CAPNP_BIN`: directory containing capnp.exe.

Replace these logical argument labels with your own existing absolute paths. No user profile, visualization directory or compiler installation is selected automatically. Rust 1.95.0 and the historical dependency locks describe the old recorded environment; a different toolchain is not qualified by those records. The helpers retain their original offline commands and component scopes. Their `lock`/`format` modes explicitly modify local component inputs; they are not production VM/controller launchers and cannot authorize privileged validation.

For example, after selecting your paths:

```text
python extensions/codex-process-control-client-v1/tool/qualify.py client fresh-label --rust-bin RUST_BIN --vs-dev-cmd VS_DEV_CMD --capnp-bin CAPNP_BIN
```

The sealed Windows coupling helper additionally requires its existing `--cargo-home` argument and the two original digest-matched guest artifacts at the repository-relative historical run locations it declares. Those Wasm/PE artifacts and build caches are not shipped by this source publication. It does not rebuild guests, grant approval, prove protected isolation, or turn a previous receipt into qualification for changed source.

Historical public validation JSON is path-redacted. See [provenance](../reports/reconstruction-2026-10-06/PUBLIC_VALIDATION_PROVENANCE.md). New invocations must use fresh labels and retain their own raw records. None of these helpers was executed as part of the path cleanup.
