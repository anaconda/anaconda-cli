"""Release stamping must not be skipped when dependencies are unchanged."""

import json
import os
import shutil
import subprocess
import sys
from pathlib import Path

import pytest

ROOT = Path(__file__).resolve().parents[1]


@pytest.mark.skipif(os.name == "nt", reason="Uses POSIX executable command stubs")
@pytest.mark.parametrize(
    ("arguments", "version", "expected"),
    [([], "", 0), (["--force"], "", 77), ([], "v1.2.3", 77)],
)
def test_ci_freshness_respects_force_and_release(
    tmp_path, arguments, version, expected
):
    scripts = tmp_path / "scripts"
    scripts.mkdir()
    shutil.copyfile(
        ROOT / "scripts/update_lockfiles.sh", scripts / "update_lockfiles.sh"
    )
    (tmp_path / "Cargo.lock").write_text("# fixture\n")
    bin_dir = tmp_path / "bin"
    bin_dir.mkdir()
    # Equal commit timestamps reproduce the CI freshness shortcut.
    git = bin_dir / "git"
    git.write_text("#!/bin/sh\nprintf '1\\n'\n")
    git.chmod(0o755)
    cargo = bin_dir / "cargo"
    cargo.write_text('#!/bin/sh\n[ "$1" = cyclonedx ] || exit 99\nexit 77\n')
    cargo.chmod(0o755)
    result = subprocess.run(
        ["bash", str(scripts / "update_lockfiles.sh"), *arguments],
        env={
            **os.environ,
            "PATH": f"{bin_dir}{os.pathsep}{os.environ['PATH']}",
            "CI": "true",
            "SBOM_RELEASE_VERSION": version,
        },
        capture_output=True,
        text=True,
    )
    assert result.returncode == expected, result.stdout + result.stderr


def test_release_version_written_when_dependencies_unchanged(tmp_path):
    target = tmp_path / "ana-x86_64-unknown-linux-gnu.json"
    shutil.copyfile(ROOT / "SBOM.json", target)
    audit = tmp_path / "audit.json"
    audit.write_text('{"vulnerabilities": {"list": []}}')
    output = tmp_path / "SBOM.json"
    markdown = tmp_path / "SBOM.md"
    command = [
        sys.executable,
        str(ROOT / "scripts/sbom-process.py"),
        "--audit",
        str(audit),
        "--output-json",
        str(output),
        "--output-md",
        str(markdown),
        str(target),
    ]
    subprocess.run(command, check=True, capture_output=True)
    before = json.loads(output.read_text())
    assert before["metadata"]["component"]["version"] == "0.0.0"
    subprocess.run(
        command + ["--release-version", "v1.2.3"], check=True, capture_output=True
    )
    after = json.loads(output.read_text())
    assert after["metadata"]["component"]["version"] == "1.2.3"
    assert after["components"] == before["components"]
    assert "ana@0.0.0" not in output.read_text()
