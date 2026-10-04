"""Repeatable plugin acceptance checks; --workspace adds repository-wide checks."""
from pathlib import Path
import argparse
import json
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[2]


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--workspace", action="store_true")
    options = parser.parse_args()
    subprocess.run([sys.executable, "scripts/plugins/build.py", "--fixtures"], cwd=ROOT, check=True)
    checks = [
        ["cargo", "test", "--locked", "-p", "nyaterm-core", "--test", "plugins"],
        ["cargo", "test", "--locked", "-p", "nyaterm-store", "plugin_preferences"],
        ["cargo", "test", "--locked", "-p", "nyaterm-plugin-host"],
        ["cargo", "test", "--locked", "-p", "nyaterm-desktop", "features::plugins"],
    ]
    if options.workspace:
        checks += [
            ["cargo", "build", "--locked"],
            ["cargo", "check", "--locked", "--workspace"],
            ["cargo", "test", "--locked", "--workspace"],
            ["cargo", "fmt", "--all", "--", "--check"],
            ["cargo", "clippy", "--locked", "--workspace", "--all-targets"],
        ]
    logs = ROOT / "target/plugin-verification"
    logs.mkdir(parents=True, exist_ok=True)
    results = []
    for index, command in enumerate(checks):
        log = logs / f"{index:02d}.log"
        print("Running " + " ".join(command), flush=True)
        with log.open("w", encoding="utf-8") as stream:
            result = subprocess.run(command, cwd=ROOT, stdout=stream, stderr=subprocess.STDOUT)
        results.append({"command": command, "exit_code": result.returncode, "log": str(log.relative_to(ROOT))})
        (logs / "results.json").write_text(json.dumps(results, indent=2) + "\n", encoding="utf-8")
        print(f"Exit {result.returncode}: {log}", flush=True)
    if any(result["exit_code"] for result in results):
        sys.exit(1)


if __name__ == "__main__":
    main()
