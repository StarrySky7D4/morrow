"""Download only missing public registry archives fixed by the audited lock.

No Cargo/user configuration is loaded. Archive checksums are checked before a
file becomes usable. Failures and partial files are retained as evidence.
"""
from concurrent.futures import ThreadPoolExecutor, as_completed
from datetime import datetime, timezone
import hashlib
import json
from pathlib import Path
import time
import urllib.request

ROOT = Path(__file__).resolve().parents[2]
INVENTORY = Path(__file__).with_name("result.json")
OUTPUT = Path(__file__).with_name("download-result.json")
CARGO_HOME = ROOT / "out/p02-native-probe-001/cargo-home"
ARCHIVES = CARGO_HOME / "registry/cache/index.crates.io-1949cf8c6b5b557f"
PARTIAL = ROOT / "out/p02-native-probe-001/dependency-downloads"
MAX_ARCHIVE = 64 * 1024**2
DEADLINE_SECONDS = 900


def now():
    return datetime.now(timezone.utc).isoformat()


def fetch(row, deadline):
    name, version = row["name"], row["version"]
    filename = f"{name}-{version}.crate"
    url = f"https://static.crates.io/crates/{name}/{filename}"
    result = dict(name=name, version=version, url=url,
                  expected_sha256=row["expected_sha256"], started_utc=now())
    if time.monotonic() >= deadline:
        return dict(result, status="not_started_budget_expired", finished_utc=now())
    path = PARTIAL / (filename + ".partial")
    try:
        request = urllib.request.Request(url, headers={"User-Agent": "MorrowCodexPinnedDependencyPreparation/1"})
        opener = urllib.request.build_opener(urllib.request.ProxyHandler({}))
        count = 0
        with opener.open(request, timeout=40) as response, path.open("xb") as output:
            if response.status != 200 or response.geturl() != url:
                raise RuntimeError("Unexpected response or redirect")
            while True:
                if time.monotonic() >= deadline:
                    raise TimeoutError("Batch elapsed-time budget expired")
                data = response.read(64 * 1024)
                if not data:
                    break
                count += len(data)
                if count > MAX_ARCHIVE:
                    raise RuntimeError("Archive byte budget exceeded")
                output.write(data)
        actual = hashlib.sha256(path.read_bytes()).hexdigest()
        result.update(bytes=count, actual_sha256=actual)
        if actual != row["expected_sha256"]:
            raise RuntimeError("Pinned Cargo.lock checksum mismatch")
        destination = ARCHIVES / filename
        if destination.exists():
            raise RuntimeError("Refusing to replace existing cache archive")
        path.replace(destination)
        result["status"] = "downloaded_checksum_verified"
    except Exception as exc:
        result.update(status="failed", error=f"{type(exc).__name__}: {exc}")
        if path.exists():
            result["partial_bytes"] = path.stat().st_size
    result["finished_utc"] = now()
    return result


def main():
    if OUTPUT.exists() or PARTIAL.exists():
        raise SystemExit("Fresh attempt required")
    inventory = json.loads(INVENTORY.read_text(encoding="utf-8"))
    pending = [r for r in inventory["archives"] if r["status"] == "missing"]
    PARTIAL.mkdir(parents=True)
    started = now()
    deadline = time.monotonic() + DEADLINE_SECONDS
    results = []
    with ThreadPoolExecutor(max_workers=8) as executor:
        futures = [executor.submit(fetch, row, deadline) for row in pending]
        for future in as_completed(futures):
            results.append(future.result())
            if len(results) % 25 == 0:
                print(json.dumps({"processed": len(results), "total": len(pending), "verified": sum(r["status"] == "downloaded_checksum_verified" for r in results)}), flush=True)
    result = dict(
        status="exact_public_dependency_preparation",
        scope=inventory["scope"],
        started_utc=started, finished_utc=now(),
        entry_sha256=hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
        inventory_sha256=hashlib.sha256(INVENTORY.read_bytes()).hexdigest(),
        deadline_seconds=DEADLINE_SECONDS, concurrency=8,
        personal_configuration_or_credentials_read=False,
        downloaded=sum(r["status"] == "downloaded_checksum_verified" for r in results),
        failed=sum(r["status"] != "downloaded_checksum_verified" for r in results),
        attempts=sorted(results, key=lambda r: (r["name"], r["version"])),
        build_passed=False,
    )
    OUTPUT.write_text(json.dumps(result, indent=2) + "\n", encoding="utf-8")
    print(json.dumps({k: v for k, v in result.items() if k != "attempts"}, indent=2), flush=True)
    raise SystemExit(1 if result["failed"] else 0)


if __name__ == "__main__":
    main()
