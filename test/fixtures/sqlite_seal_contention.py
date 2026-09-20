#!/usr/bin/env python3
"""SQLite seal-contention test helper. Usage: helper.py snapshot|lock EXISTING_DB."""
import hashlib
import json
import sqlite3
import sys
from pathlib import Path


def fail(msg):
    print(msg, file=sys.stderr)
    sys.exit(2)


def main():
    if len(sys.argv) != 3 or sys.argv[1] not in ("snapshot", "lock"):
        fail("usage: helper.py snapshot|lock EXISTING_DB")
    mode, arg = sys.argv[1], sys.argv[2]
    p = Path(arg)
    try:
        p.resolve(strict=True)
    except OSError:
        fail("db not found")
    if not p.is_file():
        fail("not a file")
    uri = p.resolve().as_uri() + ("?mode=ro" if mode == "snapshot" else "?mode=rw")
    conn = sqlite3.connect(uri, uri=True, timeout=0, isolation_level=None)
    try:
        conn.execute("BEGIN IMMEDIATE" if mode == "lock" else "BEGIN")

        rows = conn.execute(
            "SELECT payload FROM audit_identity"
        ).fetchall()
        if len(rows) != 1:
            fail("expected exactly one audit_identity row")
        identity = hashlib.sha256(rows[0][0]).hexdigest()

        pending_rows = conn.execute(
            "SELECT sequence, payload FROM outbox ORDER BY sequence"
        ).fetchall()
        h = hashlib.sha256()
        for seq, payload in pending_rows:
            h.update(seq.to_bytes(8, "little", signed=True))
            h.update(len(payload).to_bytes(8, "little"))
            h.update(payload)

        sealed = [
            hashlib.sha256(payload).hexdigest()
            for (payload,) in conn.execute(
                "SELECT payload FROM sealed_segments ORDER BY segment_index"
            )
        ]
        operations = conn.execute("SELECT count(*) FROM operations").fetchone()[0]

        print(json.dumps({
            "identity": identity,
            "pending": len(pending_rows),
            "pending_digest": h.hexdigest(),
            "sealed": sealed,
            "operations": operations,
        }), flush=True)

        if mode == "lock":
            sys.stdin.buffer.readline()  # explicit release line or parent EOF

        conn.execute("ROLLBACK")
    finally:
        try:
            conn.execute("ROLLBACK")
        except sqlite3.Error:
            pass
        conn.close()


if __name__ == "__main__":
    main()
