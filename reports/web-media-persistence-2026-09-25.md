# Web imported media persistence

Fix commit: `14421ca942b20a72e7d25766362c682de5465c1d`.
Acceptance timing correction: `a3544b66db3528ca3a8ecbf2ab2de137b7ff7660`.

## Cause and correction

Importing a background image/video or music stores its bytes in the existing
device-side `daemon-media/textures` IndexedDB store. The shared preferences codec
converted that database key using `File(key).uri`, producing a relative URI on
Web. The Rust preferences contract requires an absolute `file:` URI for local
sources and rejected the entire settings proposal, including later theme edits.
The UI consequently reported that changes remained in the current session.

The Web codec now encodes persistent database keys as
`file:///__morrow_browser_media__/<key>` and decodes them back to the exact key.
This is a virtual namespace, never an OS path or a network request. Native builds
retain their original file-path conversion. Numeric legacy keys and current UUID
keys are supported; transient object/preview URLs are rejected. Existing media
bytes are neither moved nor deleted. No server-side user storage was introduced.

The bundled workbench implementation and package version are unchanged. The fixed
client was also tested against the exact package downloaded from the previous
public deployment, verifying compatibility with its existing preferences contract.

## Verification

- Web tests: 4 passed, including actual IndexedDB media and preview ownership.
- Native path/remote URL regression: 1 passed, including Unicode and spaces.
- Rust preferences tests: 10 passed, including wire and persistent media roundtrips.
- Flutter analysis of the 6 affected Dart files: no issues.
- Production Web build: passed, including Wasm dry run.
- Real Worker/shared-controller probe: image/video/music/cover settings, subsequent
  theme edits and original media bytes survive independent Worker reopen; existing
  5 MiB attachment, stable task, draft and approval checks also passed.
- Chrome 154 and Edge 153: actual PNG/WAV/WebM file pickers, subsequent theme edits
  and two full-page reloads passed with the production application.
- Chrome with 4x CPU throttling also passed. The first CI attempt caught the test
  reloading after its fixed 1.5-second delay while a committed save still awaited
  acknowledgment. Its restored screenshot retained video, music and the white
  theme, and correctly showed "A save needs review". The harness now observes
  real Worker request/reply completion before navigating. It does not suppress
  failures, acknowledge transactions, or change application recovery behavior.
- The media scenario now gates GitHub Pages publication alongside the existing
  fresh, legacy and orphaned-data scenarios.

## Deployment

Published source: `a3544b66db3528ca3a8ecbf2ab2de137b7ff7660`.
Build and deployment both succeeded in workflow run
https://github.com/StarrySky7D4/morrow/actions/runs/36099048624.
All four CI browser scenarios passed: fresh/offline editing, legacy content,
orphaned-data protection, and media import/persistence.

Public site: https://starrysky7d4.github.io/morrow/ . HTTPS, source commit,
`/morrow/` base path, MIME types and SHA-256 hashes passed for all 36 checked assets.
The deployed workbench archive remains byte-identical to the previous release:
`b28aea5f6989967d33b6c2797624a64068da0b4d1aadc57b8db1c74cc4a79985`.
Existing package approvals therefore do not require an upgrade decision for this fix.
Chrome 154 and Edge 153 both passed the complete media import, later theme edit
and reload scenario against this public HTTPS deployment. The harness also
confirmed that the scenario generated no HTTP upload/body-bearing requests.

Changes rejected by the old version were not durably saved. If an old tab was
already closed, those selections may need to be made again after loading the fix.
