# Host-kit consumption gate

The only authority for the new generic host contract is the Morrow host session.
The plugin repository must not create a second schema to make generation or tests
pass. A review record binds to the actual delivered manifest SHA-256; naming a
directory `host-kit` is insufficient.

On concrete host handoff, perform these read-only checks:

1. Verify all declared files and hashes against the exported bytes; identify the
   schema, generated bindings, generator/tool versions, positive and negative
   vectors, and explicit implemented/draft classifications.
2. Confirm source baseline and host-owned schema path. Record experiment version,
   compatibility limits, and no SDK-freeze claim.
3. Read the consumer API and selected vectors before writing a P-02 adapter. Check
   unknown-version rejection and u64 representation against the host's evidence.
4. Write a plugin-local review receipt that pins the manifest digest and states
   `qualification_only` (or rejects it with concrete deficiencies). This status
   means suitable for the described limited probe, never production approval.
5. Copy only explicitly exported kit inputs, if required, into `sdk/host-kit/` and
   verify identical bytes. Keep the authority in the host repository. Do not edit
   generated bindings, schema, or vectors to make a plugin test pass.

`preflight --scope full` checks the reviewed digest and graph/tool/source inputs.
Its success, when attainable, still cannot claim a completed P-02 probe. Dynamic
interception must originate from a real pinned upstream call path. Fake endpoint
unit tests disconnected from that path are insufficient.

The minimal host kit may not yet implement all production network/store/permit
semantics. Report that gap without substituting a local second Store, permissive
executor, or synthetic agent loop. Do independent P-00/P-01 work first.
