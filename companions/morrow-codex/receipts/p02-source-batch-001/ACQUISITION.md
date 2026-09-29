# P02 fixed public source acquisition — batch 001

This batch is separate from the frozen P00/P01 evidence. All acquisition writes are under `upstream/p02-source-batch-001/` and `receipts/p02-source-batch-001/`. The previous `upstream/reference/`, source locks, inventories, reports, and handoff inputs were not extended or overwritten.

## Fixed identities

| Source | Fixed commit | Complete Git root tree |
| --- | --- | --- |
| `https://github.com/openai/codex` | `44fe510ce3ee61c8ef623adcbf89b901c73ddd61` | `3b868fad63be6ac5db91402b579fab37f587d7d5` |
| `https://github.com/farion1231/cc-switch` | `846de29c13ac4d65f164db8c15dd5fd58e29f972` | `e373c27485681c074ddab7d4f085ee9a6667b891` |

The fixed GitHub commit API responses preserve the commit-to-tree association. All child trees are independently recomputed using Git's canonical object encoding and byte ordering; all file contents must match the resulting tree's size and Git blob identity. Final manifests additionally record SHA-256 and Git modes for every file. This establishes a complete fixed source tree, not a fetched Git history. Commit signatures are not locally verified by this acquisition workflow.

## Failed transports are preserved as failures

The initial Git fetches used HTTP/1.1 and OpenSSL, no Git templates, disabled credential helpers/prompts, and isolated system/global configuration. Both fixed-SHA fetches returned exit 128 with `Recv failure: Connection was reset`. The exact argument arrays, UTC times, exit codes, stdout and stderr remain in `codex-git-attempt.json`, `cc-switch-git-attempt.json`, and their referenced files. The initialized but unsuccessful Git directories were preserved; they are not the usable source directories.

| Archive attempt | Start UTC | End UTC | Received bytes | Known total | Exit/status |
| --- | --- | --- | ---: | ---: | --- |
| Codex | 2026-09-28 11:39:43.886099 | 2026-09-28 11:56:50.290921 | 15,296,794 | 16,560,151 | 1 / content-length mismatch |
| CC Switch | 2026-09-28 11:40:19.419552 | 2026-09-28 11:55:33.006682 | 12,845,056 | not supplied | 1 / IncompleteRead |

The Codex failed archive is 1,263,357 bytes short of its declared content length. CC Switch's missing compressed byte count is unknown. Its exception mentions 180,224 partial bytes; those bytes were not appended, so the actual preserved file remains 12,845,056 bytes.

Preserved incomplete archive SHA-256:

- Codex: `887d6de60ed45d4349600fe5383578fd8f558e2a678ea41ff6074c8eceb218dc`.
- CC Switch: `52f3713fddacb3aa50b517eacc77cc4059c763ddb9f6b7c4bb110b8886738aa4`.

Both one-byte Range probes returned HTTP 200 rather than 206. Their bodies were not appended to the partial archives. The exact requests and headers are recorded in `codex-range-check.json` and `cc-switch-range-check.json`. No incomplete archive is represented as a successful archive or as a complete-source hash.

## Recovery and verification

`reconstruct_codex.py` recovered only complete tar members whose bytes matched independently acquired Git blobs. The truncated final member was not accepted. It recovered 8,003 files; 694 files / 5,573,906 source bytes were missing. Fixed-commit raw HTTPS supplied 691 of those files; three connection resets were preserved in `codex-raw-recovery.json`, then fixed-commit GitHub base64 file responses supplied the remaining three. `finalize_codex.py` revalidated the complete tree and physical file set using explicit failures rather than Python assertions, checked Windows drive/ADS/path boundaries and existing reparse ancestors, materialized the one safe relative symlink, and promoted the staging directory only after successful checks.

The usable Codex source is `upstream/p02-source-batch-001/codex-source/`: 8,697 files, 985 trees including the root, and 86,130,187 source bytes. `codex-source-verification.json`, `codex-content-manifest.json`, and `codex-complete-git-tree.json` are the primary machine evidence. `codex-source-recheck-001.json` is a separate read-only physical recheck, including complete directory and file sets and a fresh root-tree calculation from the actual source bytes.

CC Switch recovery has its own records and did not change the completed Codex tree or its manifests. Its independent metadata reuses the previous fixed GitHub tree only as read-only evidence and is copied into this new batch; the entire tree is recomputed against this batch's fixed commit API. The failed archive yielded 81 complete blob-verified members; 1,241 files / 33,958,908 source bytes were missing. Fixed raw HTTPS supplied 1,219 files; 22 connection resets or timeouts were retained in `cc-switch-raw-recovery.json`, then fixed-commit GitHub base64 file responses supplied all 22 missing files. `finalize_cc_switch.py` and a separate read-only recheck both passed.

The usable CC Switch source is `upstream/p02-source-batch-001/cc-switch-source/`: 1,322 files, 133 trees including the root, and 48,415,580 source bytes. There are no missing, modified, or extra source files or directories in either final source tree. `source-inventory.json` provides the two final identities and receipt hashes. The CC source-specific final evidence is `cc-switch-source-verification.json`, `cc-switch-source-recheck-001.json`, `cc-switch-content-manifest.json`, and `cc-switch-complete-git-tree.json`.

Git `100644` and `100755` modes are retained and checked as source metadata. Windows Unix execute-bit equivalence is not claimed. Source dirty state is measured as full byte/file-set equivalence to the fixed tree, not `git status`, because these materializations have no `.git` history. Codex's Apache-2.0 `LICENSE` and `NOTICE`, and CC Switch's MIT `LICENSE`, are included and hashed with all other repository files. Any other vendored license files remain present in the complete trees.

Repository completeness is not proof of an external Cargo/npm dependency closure, successful compilation, runtime isolation, or product behavior. This source-audit agent did not execute upstream programs, run Cargo, log in, read account configuration, commit, or push. Other agents' separately authorized metadata/build/probe work has its own receipts and is not negated by that statement.

## Recheck without modifying sources

Run from the plugin root. Omit `--receipt-name` for stdout only, or choose a new basename because existing receipts are never overwritten:

```powershell
python .\upstream\p02-source-batch-001\verify_materialized_source.py codex --receipt-name codex-source-recheck-002.json
python .\upstream\p02-source-batch-001\verify_materialized_source.py cc-switch --receipt-name cc-switch-source-recheck-002.json
```

This checks both SHA-256 manifests and actual Git object identities, the complete physical file and directory sets, safe symlinks/reparse points, and the fixed root tree. It does not execute source files.

## Reproducible fresh transport attempts

The following are recovery recipes, not claims of successful execution. Use unused attempt paths; never overwrite the failed archives or completed source trees. A returned file must still be fully validated against the fixed tree before use. The current batch already includes successful blob-based recovery where the primary verification receipts say complete.

```powershell
curl.exe -q --fail --location --connect-timeout 30 --max-time 1800 --output .\upstream\p02-source-batch-001\codex-archive-retry-002.tar.gz https://codeload.github.com/openai/codex/tar.gz/44fe510ce3ee61c8ef623adcbf89b901c73ddd61
curl.exe -q --fail --location --connect-timeout 30 --max-time 1800 --output .\upstream\p02-source-batch-001\cc-switch-archive-retry-002.tar.gz https://codeload.github.com/farion1231/cc-switch/tar.gz/846de29c13ac4d65f164db8c15dd5fd58e29f972
```

For a fresh isolated Git attempt, run the following in a new task-specific PowerShell process, changing only the source URL, fixed SHA and unused destination for CC Switch:

```powershell
$env:GIT_CONFIG_GLOBAL = 'NUL'
$env:GIT_CONFIG_SYSTEM = 'NUL'
$env:GIT_CONFIG_NOSYSTEM = '1'
$env:GIT_TERMINAL_PROMPT = '0'
$env:GIT_ASKPASS = ''
$env:SSH_ASKPASS = ''
git -c init.templateDir= -c init.defaultBranch=source init .\upstream\p02-source-batch-001\codex-git-retry-002
git -c credential.helper= -c core.askpass= -c http.version=HTTP/1.1 -c http.sslBackend=openssl -c core.autocrlf=false -c core.longpaths=true -C .\upstream\p02-source-batch-001\codex-git-retry-002 fetch --no-tags --depth 1 https://github.com/openai/codex.git 44fe510ce3ee61c8ef623adcbf89b901c73ddd61
git -c core.autocrlf=false -c core.longpaths=true -C .\upstream\p02-source-batch-001\codex-git-retry-002 checkout --detach FETCH_HEAD
```

Do not run the final checkout after a failed fetch. The fixed raw fallback URL is `https://raw.githubusercontent.com/<owner>/<repository>/<fixed-commit>/<percent-encoded-source-path>`; it is accepted only after expected-size and Git-blob checks. The GitHub connector fallback supplies base64 bytes and remote blob SHA for the same explicit commit. No branch head or `latest` ref is used.
