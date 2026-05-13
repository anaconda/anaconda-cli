"""Integration tests for Windows signing"""

from __future__ import annotations

import json
import os
import subprocess
from typing import TYPE_CHECKING

import pytest
from helpers import IS_WINDOWS
from helpers import get_powershell_binary

if TYPE_CHECKING:
    from pathlib import Path
    from typing import Any

    from helpers import AnaRunner

if not IS_WINDOWS:
    pytest.skip("Windows signing tests", allow_module_level=True)

if not (PWSH := get_powershell_binary()):
    pytest.skip("Tests require PowerShell.", allow_module_level=True)

# Use environment variable as sentinel variable
# since local builds are not expected to be signed
if not os.environ.get("TEST_WINDOWS_SIGNING"):
    pytest.skip("Binary is not expected to be signed", allow_module_level=True)


def get_authenticode_signature(binary_path: Path) -> dict[str, Any]:
    """Get Authenticode signature information for a Windows binary."""
    result = subprocess.run(
        [
            PWSH,
            "-c",
            # ConvertTo-Json truncates after a maximum nesting depth.
            # In PowerShell 7, this emits a warning, which makes the
            # output invalid JSON.
            f"ConvertTo-Json -Depth 5 (Get-AuthenticodeSignature '{binary_path}')",
        ],
        text=True,
        check=True,
        capture_output=True,
    )
    return json.loads(result.stdout)


def assert_signature_valid(cert_info: dict[str, Any], name: str) -> None:
    """Assert that a binary is signed with the expected certificate.

    Status codes:
      0: Signed with a trusted certificate.
      1: Signed with an untrusted certificate.
      2: Not signed
    """
    status = cert_info.get("Status", -1)
    assert status < 2, f"{name} is not signed"
    expected_status = (
        0 if os.environ.get("CERTIFICATE_TRUSTED", "").lower() == "true" else 1
    )
    assert status == expected_status, f"{name} trust status mismatch"


def test_binary_signed(ana_binary: Path | None) -> None:
    """Test whether the ana binary is signed with the expected certificate."""
    if ana_binary is None:
        pytest.skip(
            "ana binary not found. Build with 'pixi run build-release' or set ANA_BINARY_PATH"
        )
    cert_info = get_authenticode_signature(ana_binary)
    assert_signature_valid(cert_info, "ana binary")


def test_shim_signed(
    run_ana: AnaRunner,
    fake_home: Path,
) -> None:
    """Test whether the installed shim is signed with the expected certificate."""
    result = run_ana("tool", "install", "pixi")
    assert result.returncode == 0, f"Failed to install pixi: {result.stderr}"

    shim_path = fake_home / ".ana" / "bin" / "pixi.exe"
    assert shim_path.exists(), f"Shim not found at {shim_path}"

    cert_info = get_authenticode_signature(shim_path)
    assert_signature_valid(cert_info, "pixi shim")
