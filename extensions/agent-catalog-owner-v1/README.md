# Native agent catalog owner

C19 adds host-only typed `inspect_session_exec`, `install_session_exec` and
`connect_session_exec` entry points for a separately selected R2 import profile.
The existing administrative protocol and its process-profile actions are unchanged.
Installation reserves its confirmation ID before publishing and never selects,
approves or enables implicitly. The ordinary existing base/wrapper selection,
approval and enable actions remain independent decisions. Connection requires
the complete immutable wrapper review, original base selection and both current
revisions; it issues no executor authority beyond the persisted session subset.
No compilation or runtime acceptance is asserted by this source preparation.

This owner holds the original durable wrapper Catalog and trusted reserved IDs.
It borrows the application's original Manager and, for connection, its original
HostRuntime and SessionExecHost. It creates no Core Store, runtime, or replacement
connection. The Workbench must move it with its complete original owner and gate
administration on that owner's availability and maintenance state.

Installing a complete wrapper does not select or enable its base. Base selection,
base enable, wrapper selection, wrapper approval and wrapper enable are separate
explicit decisions. Base enable accepts only the exact installed base with empty
original Core and IO approval sets; session and process grants still require a
separate complete-wrapper approval. Reserved application IDs cannot be installed
or changed through this entry.

Mutating request IDs have a bounded, non-refundable lifetime index. Reuse is
rejected without execution, including after a known failure. Unknown publication
or original Manager uncertainty stops original catalog connections and prevents
further decisions or connection until a trusted owner is explicitly reopened.
The owner never retries publication or replays a request automatically.

Inspect reads one bounded local regular file and performs the original complete
wrapper review without installation. Install reads it again and verifies the
whole archive SHA before the original Catalog installs those exact bytes. Reparse
points, symlinks, network/device paths, relative paths and oversized files are
rejected. This pins the read archive bytes, not the identity of every ancestor
directory against concurrent replacement.

Tests use ordinary temporary SQLite and synthetic packages. This crate supplies
no production worker, WindowsExecutionPort binding, ProtectedSession/DPAPI,
Workbench GUI, or OS sandbox qualification. Borrowed connection is a trusted
interface, not a guest or administrative execution command.
