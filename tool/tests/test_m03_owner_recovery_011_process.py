"""Pure chain-auditor tests; these do not claim native resource qualification."""
import copy
import importlib.util
import json
from pathlib import Path
import sqlite3
import tempfile
import unittest

RUNNER = Path(__file__).resolve().parents[1] / "m03_owner_recovery_011_process.py"
if not RUNNER.is_file():
    RUNNER = Path(__file__).with_name("C011_process.py")
spec = importlib.util.spec_from_file_location("owner011_process", RUNNER)
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)


def raw(value):
    return module.wire(value).decode()


def fixture(prior="", sequence=1):
    original = {"version": 2, "phase": "Closing", "profile": "ab" * 32,
                "generation": sequence, "incarnation": "cd" * 32,
                "supervisor_sha256": "ef" * 32, "supervisor_pid": 123,
                "supervisor_creation_filetime": 99, "business_gate_revoked": True,
                "normal_shutdown": False, "business_outcome": "Unknown",
                "protocol_ack": "not_applicable", "child_exit_code": 72,
                "recovery_history_head": prior, **{key: True for key in module.PROOF}}
    text = json.dumps(original, indent=2, ensure_ascii=False)
    digest = module.sha_bytes(text.encode())
    receipt = {"version": 1, "acknowledged_unknown": True, "original_digest": digest,
               "profile": original["profile"], "generation": sequence,
               "incarnation": original["incarnation"], "manager_sha256": original["supervisor_sha256"],
               "manager_pid": 124, "manager_creation_filetime": 100, "original_supervisor_ended": True}
    receipt_text = raw(receipt)
    receipt_digest = module.sha_bytes(receipt_text.encode())
    row = [sequence, text, digest, receipt_text, receipt_digest, prior]
    row.append(module.sha_bytes(module.wire(row)))
    recovered = copy.deepcopy(original)
    recovered["phase"] = "Recovered"
    recovered["recovery"] = {"version": 1, "original_digest": digest, "receipt_digest": receipt_digest, "history_head": row[6]}
    return recovered, row


def audit(owner, rows):
    text = raw(owner)
    return module.audit_rows(text, module.sha_bytes(text.encode()), rows)


def rehash(row):
    row[2] = module.sha_bytes(row[1].encode())
    row[4] = module.sha_bytes(row[3].encode())
    row[6] = module.sha_bytes(module.wire(row[:6]))


class ChainAuditTests(unittest.TestCase):
    def test_original_raw_whitespace_is_preserved(self):
        owner, row = fixture()
        self.assertEqual(audit(owner, [row])["history_count"], 1)
        self.assertNotEqual(row[1], raw(json.loads(row[1])))

    def test_raw_text_equivalent_rewrite_is_detected(self):
        owner, row = fixture()
        row[1] = raw(json.loads(row[1]))
        with self.assertRaisesRegex(ValueError, "archived raw-text"):
            audit(owner, [row])

    def test_receipt_corruption_is_detected(self):
        owner, row = fixture()
        row[3] += " "
        with self.assertRaisesRegex(ValueError, "receipt raw-text"):
            audit(owner, [row])

    def test_all_original_resource_flags_are_required_after_rehash(self):
        for key in module.PROOF + ("business_gate_revoked",):
            owner, row = fixture()
            original = json.loads(row[1])
            original[key] = False
            row[1] = raw(original)
            rehash(row)
            with self.subTest(key=key), self.assertRaisesRegex(ValueError, "complete abnormal proof"):
                audit(owner, [row])

    def test_normal_and_protocol_ack_cannot_be_invented(self):
        for key, value in (("normal_shutdown", True), ("protocol_ack", "confirmed"), ("business_outcome", "Success")):
            owner, row = fixture()
            original = json.loads(row[1])
            original[key] = value
            row[1] = raw(original)
            rehash(row)
            with self.subTest(key=key), self.assertRaisesRegex(ValueError, "complete abnormal proof"):
                audit(owner, [row])

    def test_receipt_identity_cannot_change_even_after_rehash(self):
        for key, value in (("generation", 3), ("manager_sha256", "x"), ("manager_creation_filetime", 0), ("original_supervisor_ended", False)):
            owner, row = fixture()
            receipt = json.loads(row[3])
            receipt[key] = value
            row[3] = raw(receipt)
            rehash(row)
            with self.subTest(key=key), self.assertRaises(ValueError):
                audit(owner, [row])

    def test_two_receipts_chain_and_next_generation(self):
        _, first = fixture()
        owner, second = fixture(first[6], 2)
        self.assertEqual(audit(owner, [first, second])["history_count"], 2)
        owner.update(phase="Active", generation=3, incarnation="new", recovery_history_head=second[6])
        owner.pop("recovery")
        self.assertEqual(audit(owner, [first, second])["history_head"], second[6])

    def test_archive_deletion_and_sequence_fork_are_detected(self):
        _, first = fixture()
        owner, second = fixture(first[6], 2)
        with self.assertRaises(ValueError):
            audit(owner, [second])
        second[5] = "different"
        rehash(second)
        with self.assertRaises(ValueError):
            audit(owner, [first, second])

    def test_recovered_owner_cannot_change_unknown_or_identity(self):
        for key, value in (("generation", 2), ("incarnation", "new"), ("normal_shutdown", True), ("business_outcome", "Success")):
            owner, row = fixture()
            owner[key] = value
            with self.subTest(key=key), self.assertRaisesRegex(ValueError, "differs from literal"):
                audit(owner, [row])

    def test_read_only_sqlite_backup_preserves_literal_archive(self):
        with tempfile.TemporaryDirectory() as directory:
            source, backup = Path(directory) / "source.sqlite", Path(directory) / "backup.sqlite"
            owner, row = fixture()
            text = raw(owner)
            db = sqlite3.connect(source)
            db.execute("CREATE TABLE owner(singleton INTEGER,record TEXT,digest TEXT)")
            db.execute("CREATE TABLE owner_recovery(sequence INTEGER,original_record TEXT,original_digest TEXT,receipt TEXT,receipt_digest TEXT,previous_head TEXT,head TEXT)")
            db.execute("INSERT INTO owner VALUES(1,?,?)", (text, module.sha_bytes(text.encode())))
            db.execute("INSERT INTO owner_recovery VALUES(?,?,?,?,?,?,?)", row)
            db.commit()
            db.close()
            before = module.sha(source)
            self.assertEqual(module.read_sqlite(source, backup), module.read_sqlite(backup))
            self.assertEqual(module.sha(source), before)
            self.assertEqual(module.read_sqlite(backup)["history_rows"][0][1], row[1])

    def test_missing_v2_history_table_is_conservative(self):
        with tempfile.TemporaryDirectory() as directory:
            source = Path(directory) / "source.sqlite"
            owner, _ = fixture()
            owner["phase"] = "Closing"
            text = raw(owner)
            db = sqlite3.connect(source)
            db.execute("CREATE TABLE owner(singleton INTEGER,record TEXT,digest TEXT)")
            db.execute("INSERT INTO owner VALUES(1,?,?)", (text, module.sha_bytes(text.encode())))
            db.commit()
            db.close()
            with self.assertRaisesRegex(ValueError, "v2 recovery table missing"):
                module.read_sqlite(source)

    def test_immutable_backup_read_does_not_create_wal_sidecars(self):
        with tempfile.TemporaryDirectory() as directory:
            source, backup = Path(directory) / "live.sqlite", Path(directory) / "sealed.sqlite"
            owner, row = fixture()
            text = raw(owner)
            db = sqlite3.connect(source)
            db.execute("PRAGMA journal_mode=WAL")
            db.execute("CREATE TABLE owner(singleton INTEGER,record TEXT,digest TEXT)")
            db.execute("CREATE TABLE owner_recovery(sequence INTEGER,original_record TEXT,original_digest TEXT,receipt TEXT,receipt_digest TEXT,previous_head TEXT,head TEXT)")
            db.execute("INSERT INTO owner VALUES(1,?,?)", (text, module.sha_bytes(text.encode())))
            db.execute("INSERT INTO owner_recovery VALUES(?,?,?,?,?,?,?)", row)
            db.commit()
            try:
                module.read_sqlite(source, backup)
            finally:
                db.close()
            before = {p.name: module.sha(p) for p in Path(directory).iterdir()}
            for _ in range(2):
                self.assertEqual(module.read_sqlite(backup, immutable=True)["raw_record"], text)
            self.assertEqual(before, {p.name: module.sha(p) for p in Path(directory).iterdir()})


if __name__ == "__main__":
    unittest.main()
