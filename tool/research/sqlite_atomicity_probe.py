"""Synthetic SQLite process-crash probe, NOT an application storage backend.
No user data or production database is read. Console-only diagnostic results.
"""
import os
from pathlib import Path
import sqlite3
import subprocess
import sys
import tempfile


def connect(path):
    connection = sqlite3.connect(path)
    connection.execute("PRAGMA journal_mode=WAL")
    connection.execute("PRAGMA synchronous=FULL")
    connection.execute("PRAGMA foreign_keys=ON")
    return connection


def child(path, phase):
    connection = connect(path)
    connection.execute("BEGIN IMMEDIATE")
    connection.execute("INSERT INTO cards VALUES ('op-1', ?)", (b'\x08\x01',))
    if phase == "after_card":
        os._exit(77)
    connection.execute("INSERT INTO outbox VALUES ('event-1', 'op-1', ?)", (b'\x08\x01',))
    if phase == "before_commit":
        os._exit(77)
    connection.commit()
    os._exit(77)


def main():
    root = Path(__file__).resolve().parents[2]
    build = (root / "build" / "research").resolve()
    if not build.is_relative_to(root):
        raise RuntimeError("Probe output escapes project")
    build.mkdir(parents=True, exist_ok=True)
    print("Python SQLite version:", sqlite3.sqlite_version, flush=True)
    # This newly created directory is owned exclusively by this probe.
    with tempfile.TemporaryDirectory(prefix="sqlite-atomicity-", dir=build) as folder:
        for phase in ("after_card", "before_commit", "after_commit"):
            path = Path(folder) / (phase + ".db")
            connection = connect(path)
            connection.executescript(
                "CREATE TABLE cards(operation_id TEXT PRIMARY KEY, body BLOB NOT NULL);"
                "CREATE TABLE outbox(event_id TEXT PRIMARY KEY, operation_id TEXT UNIQUE "
                "REFERENCES cards(operation_id), body BLOB NOT NULL);"
            )
            connection.close()
            result = subprocess.run(
                [sys.executable, str(Path(__file__).resolve()), "--child", str(path), phase],
                timeout=20,
            )
            assert result.returncode == 77, result.returncode
            connection = connect(path)
            try:
                counts = tuple(connection.execute("SELECT count(*) FROM " + table).fetchone()[0]
                               for table in ("cards", "outbox"))
                expected = (1, 1) if phase == "after_commit" else (0, 0)
                assert counts == expected, (phase, counts)
                assert connection.execute("PRAGMA integrity_check").fetchone()[0] == "ok"
                print(phase + ": cards/outbox=" + str(counts), flush=True)
                if phase == "after_commit":
                    connection.execute("BEGIN IMMEDIATE")
                    existing = connection.execute(
                        "SELECT operation_id FROM cards WHERE operation_id=?", ("op-1",)
                    ).fetchone()
                    assert existing is not None
                    connection.commit()
                    counts = tuple(connection.execute("SELECT count(*) FROM " + table).fetchone()[0]
                                   for table in ("cards", "outbox"))
                    assert counts == (1, 1)
                    print("retry same operation: no duplicate commit", flush=True)
            finally:
                connection.close()
    print("3 crash points + 1 retry passed; NOT power-loss, Rust or Web durability proof")


if __name__ == "__main__":
    if len(sys.argv) > 1 and sys.argv[1] == "--child":
        child(sys.argv[2], sys.argv[3])
    else:
        main()
