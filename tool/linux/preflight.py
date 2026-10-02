#!/usr/bin/env python3
"""Read-only Linux build prerequisite inventory; never a product readiness gate."""
from __future__ import annotations

import argparse
import json
import os
from pathlib import Path
import platform
import re
import selectors
import time
import shutil
import subprocess

TOOLS = ("clang++", "cmake", "ninja", "pkg-config")
GTK_MODULES = ("gtk+-3.0", "glib-2.0", "gio-2.0")
PRODUCT_BLOCKERS = (
    "protected Linux Store and all protected reopen/backup/recovery paths unavailable",
    "product supervisor, authenticated transport and owner lifecycle integration unavailable",
    "Flutter product entrypoint unavailable; foundation runner is diagnostic GTK only",
    "real same-UID SecretService/peer/owner-churn and full GUI product qualification not completed",
)


def local_environment(root: Path | None, base: dict[str, str] | None = None) -> dict[str, str]:
    env = dict(os.environ if base is None else base)
    if root is None:
        return env
    root = root.resolve(strict=True)
    if not (root / "usr").is_dir():
        raise ValueError("toolchain root has no usr directory")
    env["PATH"] = str(root / "usr/bin") + os.pathsep + env.get("PATH", "")
    libraries = (root / "usr/lib/x86_64-linux-gnu", root / "lib/x86_64-linux-gnu")
    # Scope these variables to child processes; never modify a global profile.
    env["LD_LIBRARY_PATH"] = os.pathsep.join(map(str, libraries))
    env["PKG_CONFIG_SYSROOT_DIR"] = str(root)
    env["PKG_CONFIG_LIBDIR"] = os.pathsep.join(map(str, (
        root / "usr/lib/x86_64-linux-gnu/pkgconfig", root / "usr/lib/pkgconfig", root / "usr/share/pkgconfig")))
    return env


def bounded_stdout(command: list[str], env: dict[str, str], limit: int = 128,
                   timeout: float = 10) -> bytes | None:
    """Bound bytes retained AND work duration; refuse noisy or stuck tools."""
    process = None
    selector = selectors.DefaultSelector()
    output = bytearray()
    until = time.monotonic() + timeout
    try:
        process = subprocess.Popen(command, env=env, stdout=subprocess.PIPE,
                                   stderr=subprocess.DEVNULL)
        os.set_blocking(process.stdout.fileno(), False)
        selector.register(process.stdout, selectors.EVENT_READ)
        while True:
            remaining = until - time.monotonic()
            if remaining <= 0:
                return None
            if not selector.select(remaining):
                return None
            try:
                chunk = os.read(process.stdout.fileno(), min(4096, limit + 1 - len(output)))
            except BlockingIOError:
                continue
            if not chunk:
                remaining = until - time.monotonic()
                if remaining <= 0:
                    return None
                return bytes(output) if process.wait(timeout=remaining) == 0 else None
            output.extend(chunk)
            if len(output) > limit:
                return None
    except (OSError, subprocess.TimeoutExpired):
        return None
    finally:
        selector.close()
        if process is not None:
            if process.poll() is None:
                process.kill()
                try:
                    process.wait(timeout=1)
                except subprocess.TimeoutExpired:
                    pass
            if process.stdout is not None:
                process.stdout.close()


def package_version(executable: str, module: str, env: dict[str, str]) -> str | None:
    raw = bounded_stdout([executable, "--modversion", module], env)
    if raw is None:
        return None
    try:
        version = raw.decode("ascii").strip()
    except UnicodeDecodeError:
        return None
    # Expected pkg-config metadata only. Do not copy arbitrary tool output,
    # escape sequences, extra lines or injected JSON into the inventory.
    return version if re.fullmatch(r"[0-9]+(?:\.[0-9]+){1,3}", version) else None


def inventory(env: dict[str, str]) -> dict:
    tools = {name: shutil.which(name, path=env.get("PATH")) for name in TOOLS}
    runnable = {}
    for name, path in tools.items():
        if not path:
            runnable[name] = False
            continue
        try:
            check = subprocess.run([path, "--version"], env=env, stdout=subprocess.DEVNULL,
                                   stderr=subprocess.DEVNULL, timeout=10, check=False)
            runnable[name] = check.returncode == 0
        except (OSError, subprocess.TimeoutExpired):
            runnable[name] = False
    modules = {}
    for name in GTK_MODULES:
        if not tools["pkg-config"]:
            modules[name] = {"available": False, "version": None}
            continue
        version = package_version(tools["pkg-config"], name, env)
        modules[name] = {"available": version is not None, "version": version}
    return {
        "schema": "morrow.linux.foundation.preflight.v1",
        "platform": platform.system(),
        "machine": platform.machine(),
        "kernel": platform.release(),
        "tools": tools,
        "tools_runnable": runnable,
        "gtk_modules": modules,
        "gtk_compile_prerequisites_available": all(runnable.values()) and all(m["available"] for m in modules.values()),
        "protected_product_available": False,
        "product_blockers": list(PRODUCT_BLOCKERS),
        "display_environment_present": bool(env.get("DISPLAY") or env.get("WAYLAND_DISPLAY")),
        "no_system_install_or_settings_change": True,
    }


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--toolchain-root", type=Path)
    parser.add_argument("--json", action="store_true")
    args = parser.parse_args()
    result = inventory(local_environment(args.toolchain_root))
    if args.json:
        print(json.dumps(result, indent=2))
    else:
        print("GTK compile prerequisites: " + ("available" if result["gtk_compile_prerequisites_available"] else "missing"))
        print("Protected Linux product: unavailable")
        for name, path in result["tools"].items():
            print(f"  {name}: {path or 'missing'}")
        for name, state in result["gtk_modules"].items():
            print(f"  {name}: {state['version'] or 'missing'}")
    return 0 if result["gtk_compile_prerequisites_available"] else 2


if __name__ == "__main__":
    raise SystemExit(main())
