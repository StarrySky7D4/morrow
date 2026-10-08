# Offline qualification preparation tools

Experimental internal validation primitives; these are not a frozen SDK, application entry point or permission to acquire dependencies. Source files preserve the original tested bytes; see [provenance](source-provenance.json). Private control wrappers and raw qualification records are not distributed.

`git_objects/offline_pack.py` accepts in-memory synthetic PACK data and validates bounded object/delta/tree relationships. Its CLI refuses with exit 78. SHA-1 object identity does not establish collision resistance, real repository acquisition or complete history.

`registry/install_registry.py` contains bounded archive and fresh-cache primitives. Its CLI refuses with exit 78. Unit tests create new `synthetic-*` directories alongside their copied source. Run them only in a new disposable directory; existing roots are rejected and retained. The separate historical Cargo offline metadata result covers a toy registry layout, not a real dependency closure, compiler or build script.

`capture/capture_runtime.py` only defines functions at import. Calls require the caller's authority and retain the same returned child on an unknown result; it does not grant authority or prove original-owner cleanup. `capture/synthetic_child.py` is a test fixture: no arguments, 128 KiB per output stream, a short wait and exit 0. Capture files do not have an exit-78 CLI gate. The private ten-method and loader tests are summarized with their original identities; they were not rerun here.

For Python 3.11 or later, copy each complete test directory to a new writable directory, then run `python -B test_offline_pack.py` inside the copied `git_objects` directory or `python -B test_offline.py` inside the copied `registry` directory. Tests use only ordinary synthetic data and no Git, Cargo, network or VM operations. The original registry test writes its fixtures next to the copied source; do not run it in a frozen or read-only source tree.

The publication relocation checks repeat the same 31 PACK and 12 registry methods. They are compatibility checks, not additional unique method counts. See [current results and limitations](../../../../reports/reconstruction-2026-10-09/windows-agent-sdk-c28-offline-tools.md).
