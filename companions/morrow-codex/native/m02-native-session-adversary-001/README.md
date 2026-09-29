# Test peer only

Separate executable and package from the real client. Consumes the same immutable
Cap'n Proto kit. It never creates approval or host state and cannot execute project
commands or access accounts/providers. The host launches it under an explicit
test-only admission and expected executable digest.

`--scenario wrong-pid|modified-epoch|wrong-artifact` changes one echoed Hello field;
these are synthetic bad-binding cases, not proof of replaying an actual prior
session. `unsupported-kind` sends a server-only State kind as a request after
Welcome. The peer exits0 only after observing Denied/Stop; the independent test
must still check the host's exact denial and ownership outcomes.

`no-hello`, `partial-frame`, `flood-invalid`, `ignore-stop` intentionally stall or
send malformed input so the real host's timeout/control/termination is tested.
They stop after8s or a10s hard watchdog if the host fails to terminate them.
`hold-output` handshakes, reads State, sends Close, then on valid Stop starts only
this same fixed test executable as a 1.5s stdout/stderr handle holder and exits.
The host can observe parent exit without EOF and must not invent confirmed release.
This scenario intentionally tests the disclosed descendant-containment limitation;
the child is bounded and never launches any other command.

No negative scenario has run until a real frozen host starts this peer. Its exit0
does not establish denial correctness or successful release without host/OS evidence.
