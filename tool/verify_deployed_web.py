"""Verify the public Pages release and integrity of its executable assets."""
import argparse
from concurrent.futures import ThreadPoolExecutor
import hashlib
import json
from pathlib import Path
import urllib.request
from urllib.parse import urljoin, urlparse

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("--site", default="https://starrysky7d4.github.io/morrow/")
parser.add_argument("--commit", required=True)
parser.add_argument("--output", type=Path, default=Path("build/pages-http-acceptance.json"))
args = parser.parse_args()
site = urlparse(args.site)
if site.scheme != "https" or not site.path.endswith("/"):
    parser.error("site must be an HTTPS directory URL")

def fetch(name):
    request = urllib.request.Request(urljoin(args.site, name), headers={"Cache-Control": "no-cache"})
    with urllib.request.urlopen(request, timeout=90) as response:
        if urlparse(response.url).scheme != "https":
            raise ValueError("HTTPS downgrade")
        return response.read(), response.headers.get_content_type()

release = json.loads(fetch("release.json")[0].decode("utf-8-sig"))
if release["commit"] != args.commit or release["basePath"] != site.path:
    raise ValueError(f"Unexpected deployed release: {release}")
index = fetch("index.html")[0].decode("utf-8")
if f'<base href="{site.path}">' not in index:
    raise ValueError("Wrong production base path")
sums = dict(line.split("  ", 1)[::-1] for line in fetch("SHA256SUMS.txt")[0].decode("utf-8-sig").splitlines())
names = ["index.html", "flutter_bootstrap.js", "flutter.js", "main.dart.js", "manifest.json",
         "workbench/host-worker.mjs", "workbench/library-worker.mjs", "workbench/device-identity.mjs",
         "workbench/workbench.morrowplugin", "workbench/core/morrow_web_core.js",
         "workbench/core/morrow_web_core_bg.wasm", "SOURCE.txt", "LICENSE", "NOTICE", "THIRD_PARTY_NOTICES.txt"]
names += [name for name in sums if name.startswith("canvaskit/") and name.endswith((".js", ".wasm"))]
names += [name for name in sums if name.endswith(".mlang")]

def verify(name):
    data, mime = fetch(name)
    digest = hashlib.sha256(data).hexdigest()
    if digest != sums[name]:
        raise ValueError(f"Hash mismatch: {name}")
    if name.endswith(".wasm") and mime != "application/wasm":
        raise ValueError(f"Wrong Wasm MIME: {name}: {mime}")
    if name.endswith((".js", ".mjs")) and mime not in ("text/javascript", "application/javascript"):
        raise ValueError(f"Wrong JavaScript MIME: {name}: {mime}")
    return {"path": name, "bytes": len(data), "mime": mime, "sha256": digest}

with ThreadPoolExecutor(max_workers=6) as executor:
    files = list(executor.map(verify, names))
args.output.parent.mkdir(parents=True, exist_ok=True)
args.output.write_text(json.dumps({"site": args.site, "release": release, "files": files}, indent=2), encoding="utf-8")
print(f"PASS: HTTPS, source commit, base path, MIME and SHA256 for {len(files)} deployed assets")
