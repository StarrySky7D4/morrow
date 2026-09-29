# M-02 admission/owner slice: plugin handoff

The ordinary client001 is reused without rebuilding. Its executable SHA256 is
`068122a87c1bbfe6cbac42686aa7e9abb2c520b54075de47c1d5bb0da6eb525b`, with manifest
`9448e8167737df2ec706999c66a13ce4ce400e2cbcf5aceace1c30a8361058ee`.
The authoritative guest wire remains Capnp kit001, major2/revision1; schema
`fa419bc52779e160985bf55da8ef05b7ed5f1603ebf2350d412473775ffa6450` and generated
binding `2475cee0b5cc676fe36c637daf79eb0fafd4ac378478ac7c076612fe76bc5fc9` are unchanged.

The separate replay peer001 is frozen under `native/m02-admission-owner-002`.
Its executable SHA256 is `3a12d44cfc19aaacc1a5b2f73bd4c3e516e16879b305165e39bda94a1f5f7bf6`;
candidate manifest SHA256 is `8851b4b05d3f1fe5ffec21bcfe8eeaf67f35b55573e9e95a12b45a25f8e396b5`.
The candidate binds 83 inputs; local checks passed three unit cases and four
offline CLI cases. These local checks do not establish host authorization.

This handoff pairs only the final host runtime-kit-002, manifest
`ee257b1cc15dcb72a4f92343406431fbc4a35153915e23e5f73e9265917e373e`, executable
`59b1427cb8c50ebbc50184411bb9520f8114253e8c114d83c170c8e6015b04b5`.
The incomplete host kit001 is not consumed. The host-producer result
`peer-20260928T203125991615Z/result.json` has SHA256
`0a032c8ce8a43f8839925f8bc07c4f0bcc2c360d872d3b41eea1e8465c600320`.

Four real producer scenarios completed: capture, historical Hello replay,
historical Query replay after a new valid handshake, and output holding.
The captured seven frames match the host channel capture byte-for-byte. Both
historical replays sent exactly the old accepted bytes and received identity
denial17. Host exit2 is asserted by the sealed producer harness; peer exit0 means
it correctly observed rejection and does not mean admission succeeded.
Distinct host PIDs appear in the four proposal records. The producer harness
waits for first-host exit before starting the next host; all four snapshots record
actual child exit and both EOFs before Released. The holder case records
ClosingUnconfirmed before release. These are producer observations, reviewed
read-only here, not an independent plugin-side OS witness or repeated run.

`host-pair-readonly-001.json` binds that pairing, verifies 931 unique inputs and
correlates the historical peer files with actual host channel bytes. It does not
independently decode Capnp. Its SHA256 is
`ae5f819194c7e544c613e0dfb0b7763af6fbec573eaa0b62f7d878202f97573d`.

The peer has no approve/claim API and cannot create an authority. Host-authorized
arguments select explicit test modes only. Raw test frames remain in fresh,
ACL-restricted material directories; no actual user secrets were used. The peer
waits150ms after Challenge for independent observation, counted inside the original
host deadline, and has a fixed10s watchdog. The holder is only the same executable,
starts with CREATE_NO_WINDOW and exits after2s. `holder.json` provides an owned
child PID for independent lookup; it is not itself OS identity or exit proof.

The plugin material-preparation tool writes only this plugin batch's output tree.
The joint reviewer has a separate authorized output scope and uses its own local
preparation tool and ACL receipt. That does not require changing this frozen
candidate. The initial candidate's pending text is its historical publication
state; the producer pairing above is now complete. Joint independent runtime
acceptance remains pending at this handoff and must have its own sealed receipt.

No role authorization model or A/B identity-interchange result was invented.
Different argv/config using the same executable is not strong artifact isolation.
The host scope remains one trusted same-user local profile/slot. Production
approval UI and CLI authentication, trusted installation, cross-profile/global
ownership, full crash recovery, TOCTOU/OS isolation, in-flight partial writes,
control saturation and business backend integration remain unqualified.
M-02 is partial; G0/P-02/J-00 are blocked, G1 is not passed, product graphs remain
0/2 and all original84 product cases remain not_run. No commit, push, release or
next implementation batch has been started.
