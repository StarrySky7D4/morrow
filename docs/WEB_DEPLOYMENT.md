# GitHub Pages

Production URL: https://starrysky7d4.github.io/morrow/

The public site serves static Flutter, Rust/Wasm, and bundled plugin assets.
The browser executes the core locally. Workspace records and attachment
originals live in OPFS; device identity lives in IndexedDB. GitHub does not
receive workspace contents. Browser storage is scoped to the site origin and
browser profile; clearing site data removes it. Full Windows feature parity
is tracked separately in [WEB_PARITY.md](WEB_PARITY.md).

`.github/workflows/pages.yml` builds main on Windows with pinned Flutter,
Rust, LLVM, Capn Proto, and wasm-bindgen versions. It verifies the real UI
(new library, attachment save/reload/download, legacy content, orphaned data)
before uploading the static artifact and deploying through GitHub Pages.
Repository Settings → Pages must use **GitHub Actions**. Deployment requires
the `github-pages` environment to permit main. A failed build or acceptance
does not replace the deployed site. The workflow can also be run manually.

For local reproduction, install the versions in `tool/setup_web_ci.ps1`,
resolve Flutter dependencies, then run:

```powershell
flutter pub get --enforce-lockfile
./tool/build_web_workbench.ps1 -BaseHref /morrow/
node tool/test_core_browser.mjs --app --base-path /morrow/
node tool/test_core_browser.mjs --app --base-path /morrow/ --legacy
node tool/test_core_browser.mjs --app --base-path /morrow/ --orphan
./tool/package_web_release.ps1
```

`CHROME_BIN` can select Chrome/Chromium. The harness always creates its own
browser profile and test files under ignored `build/`; it never uses the
operator's browser profile. Post-deployment acceptance runs the same commands
with `--site https://starrysky7d4.github.io/morrow/` instead of `--base-path`.
`build/web-network-*.json` records page network requests for inspection.
Add `--offline-edits` to the fresh UI scenario to disconnect the loaded page
while creating a card, importing an attachment, and saving, then reconnect
for page reload. This checks local execution; it does not claim cold startup
without an Internet connection. Validate deployed asset hashes and MIME types
with `python tool/verify_deployed_web.py --commit <deployed-source-commit>`.

`release.json` identifies the source commit. `SOURCE.txt`, `LICENSE`,
`THIRD_PARTY_NOTICES.txt`, `licenses/`, and `SHA256SUMS.txt` accompany each
release. The corresponding source is the public commit linked in the release.
To roll back, revert the offending source change on main and rerun the workflow;
do not delete or reset users' browser storage.
