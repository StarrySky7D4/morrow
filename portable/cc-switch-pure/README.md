# Pinned CCSwitch pure preparation

The original model_capabilities.rs is copied byte for byte from CCSwitch
846de29c13ac4d65f164db8c15dd5fd58e29f972. Its real context-marker definition
is extracted from claude_desktop_config.rs:39, not replaced with a mock.
source-receipt.json pins both sources and the original product lock.

The bounded v1 JSON wrapper retains Supported, Unsupported and Unknown.
Unknown is successful incomplete knowledge; parse/runtime failure is separate.
The guest sibling uses existing Morrow ABI2 Transform, no content, I/O,
network, dependency calls or native fallback. Nested settings are invocation
data only; send a model catalog subset, never provider credentials/config.

This is preparation and Morrow execution evidence, not the CCSwitch product
Wasm call path, native plugin isolation, full G0 qualification or SDK freeze.
The original Tauri callers remain unchanged. Public broker integration needs
an explicit transport/authentication design before modifying that boundary.
