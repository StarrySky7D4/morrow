# Track A work plan

Authoritative next implementation sequence after `0.1.9-test.50+55`.
This file does not mark any IO capability complete.

## W1 io-v1 contract (this change)

- Independent IO schema, declaration, and package feature `io-v1`.
- Old guest-v1-rc1 packages stay loadable and must not gain IO.
- Declaring `io-v1` without a host broker is Unsupported, never silent ignore.
- Runtime still rejects the `morrow_io_v1.call` import until W2 attaches a broker.
- No selected-file reads, HTTP send, WASI, or path grants in this package.

## Later packages

- W2 selected-file chunked read
- W3 authorized HTTPS GET + read-only recording
- W4 durable intent before any write
- W5 file write + HTTP send minimum set
