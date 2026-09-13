"""Create explicit upgrade-test candidates; never change frozen compatibility originals."""
from pathlib import Path
import subprocess
import tempfile
import verify_plugin_sdk_baseline

ROOT = Path(__file__).resolve().parents[1]


def main():
    verify_plugin_sdk_baseline.verify()
    build = ROOT / "build"
    build.mkdir(exist_ok=True)
    with tempfile.TemporaryDirectory(prefix="plugin-upgrade-candidates-", dir=build) as directory:
        for version, language, suffix in (("1.0.0", "rust", "v1"), ("1.1.0", "c", "v2")):
            candidate = Path(directory) / (suffix + ".mplugin")
            source = ROOT / "sdk/compat/guest-v1-rc1" / (language + "-ui.wasm")
            subprocess.run(["cargo", "run", "--locked", "--offline", "--release", "--manifest-path",
                str(ROOT / "core/Cargo.toml"), "--example", "plugin_package", "--", "pack-v2",
                str(source), str(candidate), "org.example.upgrade-ui", version,
                "--handler", "ui.form", "text.utf8", "morrow.ui.document.v1", "32", "65536",
                "--handler", "ui.edit", "morrow.ui.event.v1", "morrow.ui.document.v1", "65536", "65536"],
                cwd=ROOT, shell=False, check=True, timeout=900)
            destination = build / ("test50-upgrade-" + suffix + ".mplugin")
            data = candidate.read_bytes()
            if destination.exists():
                if destination.is_symlink() or destination.read_bytes() != data:
                    raise RuntimeError("existing qualification candidate differs; preserving it: " + str(destination))
            else:
                with destination.open("xb") as output:
                    output.write(data)
            print("UPGRADE CANDIDATE", destination)
    verify_plugin_sdk_baseline.verify()


if __name__ == "__main__":
    main()
