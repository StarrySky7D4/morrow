# M-02 native session client — bounded ready handoff

Implemented a real native client that consumes the host-owned Cap'n Proto kit001,
not a replacement host or approval fixture. The fixed `client-001` executable has
completed real host-produced handshake/read/close, online revoke/Denied19, and
active Stop pairings. The final host runtime identity and latest pair evidence
are bound in `handoff.json`. Joint independent runtime acceptance is separate.

- Ordinary client: `candidate-001.json`, executable SHA256
  `068122a87c1bbfe6cbac42686aa7e9abb2c520b54075de47c1d5bb0da6eb525b`.
  Its 56 source/build/kit/artifact inputs remain frozen.
- Formal kit: host manifest `2a0146a8f18b658b906c6a5f2d0334272a794edf43b0b6ac5757d9ba45cb66d9`,
  raw schema `fa419bc52779e160985bf55da8ef05b7ed5f1603ebf2350d412473775ffa6450`.
  Locally regenerated bindings match the host's fixed generated output.
- Final host: runtime-kit-001 executable SHA256
  `12ddcb1acc95a151aa52541d93c13756f50174d9f996a155ece4e0e8d7642e21`.
  Latest producer pair: `pair-20260928T193003821828Z/result.json`, SHA256
  `8041af58823981004e2a88a41e726d003ff7769fcc277022474cc67c7f169dc6`.
  `host-pair-readonly-002.json` checks this final pairing, not the earlier host build.

Nine local unit/vector-consumption tests and six offline CLI cases passed; the
host launch flag without a host pipe also returned exit24 with zero stdout bytes.
These local checks are not IPC evidence. Real pair evidence has child exits0/19/0,
host exits0, generation2, independently recorded child wait plus both output EOFs,
and Released; the control stdin remained open until host exit. No state_read occurs
after the revoke acknowledgement. These are host-producer observations reviewed
read-only here, not a separate plugin-side process replay or joint acceptance.

The client keeps initial admission bindings, validates response identity/sequence
and monotonic observations, uses bounded framing and separate bounded I/O queues,
and handles Stop during waits. Partial-frame/operation deadlines never renew;
there is also an immutable60s local lifetime cap. Failure/EOF/Denied never selects
a default backend or replays a request. stdout is protocol only. Only read status
is supported; no successful HTTP, content, store writer or arbitrary command exists.

The separate adversarial test peer is not the ordinary client. peer001 remains
frozen and unrun here. Joint requested explicit hidden creation for its bounded
same-executable output-holder descendant; peer002 adds CREATE_NO_WINDOW in a new
source/output/manifest directory. peer002 SHA256
`b5794c09973f304d70298290f7ceb29a1d3af27dba08920b87d3c06497150878` is compiled and
ready for selected missing negative cases, not itself a passed runtime case.
It never creates admission. No redundant negative-case expansion is requested.

Preserved deviations: the first host custom binary wire was withdrawn by the
main coordinator for violating the Cap'n Proto baseline; its verified copy stays
unused and is not a Cargo dependency. The first Capnp client lock attempt exposed
nested workspace roots; only the client workspace exclusion was corrected, without
changing the authoritative kit. All failed/preparatory receipts remain available.
The historical runner status prefix `passed_bootstrap_*` denotes local stages;
the candidate manifest binds the actual final Capnp source and executable.

Remaining limits: full M-02 trusted installation/approval UI, image-load race,
delegated handles/descendant containment, host crash or cross-process ownership,
OS isolation, Linux/device behavior and Core business integration are unproved.
OS write-backpressure/partial-write cancellation and blocked operator stdout are
not dynamically qualified by this slice. Exact Capnp nesting/traversal thresholds
were not exercised. Exit0 is not business success; client Stop is not host release.
ClosingUnconfirmed/owner retention requires host and independent evidence.
M-03/M-04/M-06/M-08 and G0/P02/J00 remain open; full product graphs0/2 and84not_run
are unchanged. No commit, push, release, real account/library or provider was used.
