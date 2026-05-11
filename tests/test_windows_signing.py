"""Integration tests for Windows signing"""

from __future__ import annotations

import json
import os
import subprocess
from typing import TYPE_CHECKING

import pytest
from helpers import IS_WINDOWS

if TYPE_CHECKING:
    from pathlib import Path
    from typing import Any

if not IS_WINDOWS:
    pytest.skip("Windows signing tests", allow_module_level=True)


# Use environment variable as sentinel variable
# since local builds are not expected to be signed
if not os.environ.get("WINDOWS_CERTIFICATE_FINGERPRINT"):
    pytest.skip("Binary is not expected to be signed", allow_module_level=True)


@pytest.fixture(scope="class")
def certificate_info(ana_binary: Path | None) -> dict[str, Any]:
    """Get codesign information for the binary."""
    if ana_binary is None:
        pytest.skip(
            "ana binary not found. Build with 'pixi run build-release' or set ANA_BINARY_PATH"
        )

    result = subprocess.run(
        [
            "powershell",
            "-c",
            f"ConvertTo-Json (Get-AuthenticodeSignature '{ana_binary}')",
        ],
        text=True,
        check=True,
        capture_output=True,
    )
    return json.loads(result.stdout)


class TestWindowsSigning:
    """Tests for Windows code signing verification."""

    def test_binary_signed(
        self,
        certificate_info: dict[str, Any],
    ) -> None:
        """Test whether the binary is signed.

        Status codes:
          0: Signed with a trusted certificate.
          1: Signed with an untrusted certificate.
          2: Not signed
        """
        status = certificate_info.get("Status", -1)
        assert status < 2, "Binary is not signed"
        expected_status = (
            0 if os.environ.get("CERTIFICATE_TRUSTED", "").lower() == "true" else 1
        )
        assert status == expected_status

    def test_certificate_fingerprint(
        self,
        certificate_info: dict[str, Any],
    ) -> None:
        """Verify the signature fingerprint (thumbprint)."""

        certificate = certificate_info.get("SignerCertificate")
        assert certificate, "No certificate found"
        fingerprint = certificate.get("Thumbprint", "")
        assert fingerprint == os.environ.get("WINDOWS_CERTIFICATE_FINGERPRINT")
