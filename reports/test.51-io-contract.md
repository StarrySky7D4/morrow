# test.51 IO contract (W1)

Date: 2026-09-15
Branch: `track-a/w1-io-contract`

## Done

- Frozen IO runtime frame `core/schemas/io.capnp`
- Frozen package declaration `core/schemas/io_manifest.proto`
- Feature name `io-v1` with declaration + schema digest required
- Unsupported response helper for a missing broker
- Runtime import `morrow_io_v1.call` remains rejected

## Not done

- Selected file reads
- HTTP GET/send backends
- Registry schema v2 IO approval persistence
- Opening `morrow_io_v1.call` on the runner
- Changing frozen guest-v1-rc1 originals

## Limits

This package only proves the contract and closed-door rejection.
`network_node` loopback is not guest IO.
The complete SDK is still unstable.
