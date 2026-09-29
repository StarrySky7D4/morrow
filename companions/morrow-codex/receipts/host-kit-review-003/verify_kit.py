"""Read-only verification of the host's explicitly delivered qualification kit."""
import hashlib
import json
from pathlib import Path
import stat

HERE = Path(__file__).resolve().parent
KIT = Path(r"C:\Users\Administrator\Desktop\CodeXProjext\morrow\build\io-safety-refactor\reports\codex-morrow-v1.1\host\host-kit-003")
EXPECTED_MANIFEST = "5555751ab1cf590d68e9610dae396677b79422999ec61b7c3d3d7183e8ed4a01"
EXPECTED_SCHEMA = "da0ac7a42e4b0f6aec0cfbdd2358cf86688b08e862626186bb9de4914f355a7f"
if (HERE / "byte-review.json").exists():
    raise SystemExit("Review receipt exists; do not overwrite")


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


manifest = KIT / "manifest.json"
assert digest(manifest) == EXPECTED_MANIFEST
doc = json.loads(manifest.read_text(encoding="utf-8"))
assert doc["status"] == "complete" and doc["qualification_only"] is True
assert (doc["wire_major"], doc["wire_revision"]) == (1, 1)
assert doc["schema_sha256"] == EXPECTED_SCHEMA
seen = set()
checked = []
for entry in doc["files"]:
    name = entry["path"]
    assert name not in seen
    seen.add(name)
    path = KIT / name
    assert not Path(name).is_absolute()
    path.resolve().relative_to(KIT.resolve())
    for component in [path, *path.parents]:
        if component == KIT.parent:
            break
        info = component.lstat()
        assert not stat.S_ISLNK(info.st_mode)
        assert not getattr(info, "st_file_attributes", 0) & 0x400
    assert path.stat().st_size == entry["bytes"]
    assert digest(path) == entry["sha256"]
    checked.append({"path": name, "sha256": entry["sha256"]})
actual = {p.relative_to(KIT).as_posix() for p in KIT.rglob("*") if p.is_file()}
assert actual == seen | {"manifest.json"}
assert digest(KIT / "agent_host.capnp") == EXPECTED_SCHEMA
assert digest(Path(doc["authority"]) / "agent_host.capnp") == EXPECTED_SCHEMA
assert doc["baseline"]["head"] == "88557916aabf2e10619b1022110035178498898a"
assert doc["baseline"]["branch"] == "codex/io-safety-refactor"
assert "LICENSE" in seen
vectors = json.loads((KIT / "vectors/manifest.json").read_text(encoding="utf-8"))
assert vectors["qualification_only"] is True and vectors["schema_sha256"] == EXPECTED_SCHEMA
for vector in vectors["vectors"]:
    for kind in ("request", "reply"):
        assert digest(KIT / "vectors" / vector[kind]) == vector[kind + "_sha256"]
for vector in vectors["rejections"]:
    assert digest(KIT / "vectors" / vector["request"]) == vector["sha256"]
assert digest(manifest) == EXPECTED_MANIFEST
result = {"schema_version": 1, "status": "byte_verified_pending_consumer",
          "manifest_sha256": EXPECTED_MANIFEST, "kit_path": str(KIT),
          "schema_sha256": EXPECTED_SCHEMA, "files_verified": len(checked),
          "manifest_stable": True, "vector_pairs_verified": len(vectors["vectors"]),
          "rejection_vectors_verified": len(vectors["rejections"]),
          "implemented_families": doc["implemented_families"],
          "draft_families": doc["draft_families"],
          "manual_review": ["host-owned schema/API/source and exact UInt64/version/direction checks reviewed",
                            "qualification-only fake feature; no production Store/network/executor claimed",
                            "Transport, frame/encode/decode/exchange_checked are consumer entry points"],
          "verification_limit": "Byte integrity and source review; host Rust tests were not rerun by plugin",
          "P-02": "blocked_complete_buildable_upstream_missing", "G0": "blocked",
          "files": checked}
(HERE / "byte-review.json").write_text(json.dumps(result, indent=2) + "\n", encoding="utf-8")
print(json.dumps({"status": result["status"], "files_verified": len(checked),
                  "vector_pairs_verified": len(vectors["vectors"]), "G0": "blocked"}))
