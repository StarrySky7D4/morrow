# GitHub Pages

Production URL: https://starrysky7d4.github.io/morrow/

The public site serves static Flutter, Rust/Wasm, and bundled plugin assets.
The browser executes the core locally. Workspace records and attachment
originals live in OPFS; device identity lives in IndexedDB. GitHub does not
receive workspace contents. Browser storage is scoped to the site origin and
browser profile; clearing site data removes it. Full Windows feature parity
is tracked separately in [WEB_PARITY.md](WEB_PARITY.md).

`.github/workflows/pages.yml` builds main and pull requests on Windows with pinned Flutter,
Rust, LLVM, Capn Proto, and wasm-bindgen versions. It verifies the real UI
(new library, attachment save/reload/download, legacy content, orphaned data)
before uploading the static artifact and deploying through GitHub Pages.
Repository Settings → Pages must use **GitHub Actions**. Deployment requires
the `github-pages` environment to permit main. A failed build or acceptance
does not replace the deployed site. The workflow can also be run manually.
Pull-request builds have read-only repository permission and cannot upload a
Pages artifact or run the deployment job. Both operations additionally require
the main ref. Concurrency is separated by workflow/ref, so PR validation does
not cancel a production run.

The full theme lifecycle runs with explicit 4× CPU throttling, including all
reload, lost-receipt, enable/disable/uninstall, content and media assertions.
Predicate waits use the same CPU rate: the normal local 45-second budget becomes
180 seconds at 4×, with a hard 180-second cap. Original-budget crossings are
still logged with fixture transport/UI state. The whole theme step is limited
to 25 minutes; the overall build remains limited to 60 minutes. Only text/JSON
diagnostics are uploaded, including per-operation timing checkpoints. See the
[measured repair record](../reports/web-ci-2026-10-05.md).

For local reproduction, install the versions in `tool/setup_web_ci.ps1`,
resolve Flutter dependencies, then run:

```powershell
flutter pub get --enforce-lockfile
./tool/build_web_workbench.ps1 -BaseHref /morrow/
node tool/test_core_browser.mjs --app --base-path /morrow/
node tool/test_core_browser.mjs --app --base-path /morrow/ --legacy
node tool/test_core_browser.mjs --app --base-path /morrow/ --orphan
node tool/test_core_browser.mjs --app --base-path /morrow/ --media
node tool/test_core_browser.mjs --app --base-path /morrow/ --theme --slow-ui 4
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
