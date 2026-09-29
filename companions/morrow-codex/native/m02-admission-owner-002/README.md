# M-02 admission/owner slice: separate evidence peer

This is a test artifact, never a production client or an approval authority.
The ordinary `m02-native-session-001` client and all old candidates remain frozen.
This peer consumes the unchanged host-owned capnp-kit-001; it defines no wire.

Arguments after the host-added `--morrow-native-session-v2`:

* `--scenario capture --evidence-dir ABS`: read a real Challenge; save the exact
  sent Hello, received Welcome, sent Query, received State, sent Close and Stop.
  Only a validated successful conversation writes `complete.txt` with byte sizes
  and hashes. It is not proof of process exit, pipe EOF or a trustworthy host.
* `--scenario replay-hello --evidence-dir ABS --capture-dir OLD`: after a new
  real Challenge, send the saved Hello bytes unchanged, expecting identity error17.
* `--scenario replay-query --evidence-dir ABS --capture-dir OLD`: handshake with
  the new real host normally, then send saved Query bytes unchanged, expecting17.
* `--scenario hold-output --evidence-dir ABS`: handshake/read/Close, then after
  Stop launch only this same executable in a fixed 2-second output-holder mode,
  with CREATE_NO_WINDOW and inherited output pipes. Write `holder.json` containing
  root/holder PIDs from the owned Child handle. This is a locator for independent
  OS verification, not an independent identity/exit witness. No process scan.

All modes verify the host schema and own PID/artifact digest. The explicit test
observation window is 150ms after Challenge, counted inside original host expiry;
the peer never requests renewal. Choose handshake >=500ms for these identity
scenarios; cover short deadlines separately. A fixed 10-second process watchdog
exits97 without retries. The holder exits itself after2s. No arbitrary descendant,
provider, account, business command, network or content store is available.

Use `prepare_materials.ps1` before launching: it creates fresh ACL-protected local
test directories in this batch's output tree (current user and SYSTEM only).
Do not place real user secrets in them. Fixed-name writes use create_new and are
bounded; the peer refuses non-empty destinations and reparse paths. This is not
an OS sandbox or protection against the same-user attacker replacing a file.
Raw Capnp frames include test admission correlation material and stay in those
restricted directories; do not print them or copy them into unrestricted reports.
Metadata receipts contain no nonce/full frame. The peer never removes materials.

The trusted host/harness alone owns approve/claim/revoke. The new host starts as
a proposal and requires an explicit operator-side approval; no such API exists
in this peer. The trusted reviewer must separately prove the first host/child
exit and both child output EOFs before starting a replay host, record both host
identities, compare peer bytes to host channel capture, and decode independently.
Neither a capture file nor a success exit code grants authority or proves that
the peer's report is truthful. Natural argv/config, PID/session/nonce changes are
reported; same binary with different argv is not strong artifact isolation.

Roles/plugin identities are solely host context. This peer does not invent a
role field or claim identity interchange coverage. This slice does not complete
production approval UI/CLI authentication, hostile installation attestation,
cross-profile global ownership, crash recovery or business backend acceptance.
