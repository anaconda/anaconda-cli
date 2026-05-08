"""Integration tests for macOS code signing."""

from __future__ import annotations

import os
import subprocess
import tempfile
from pathlib import Path

import pytest
from cryptography import x509
from cryptography.hazmat.primitives import hashes
from helpers import IS_MACOS

if not IS_MACOS:
    pytest.skip("macOS signing tests", allow_module_level=True)


def get_signing_cert_fingerprint(binary_path: Path) -> str:
    """Extract SHA-256 fingerprint of the signing certificate."""
    with tempfile.TemporaryDirectory() as tmpdir:
        subprocess.run(
            ["codesign", "-d", "--extract-certificates", str(binary_path.resolve())],
            cwd=tmpdir,
            check=True,
            capture_output=True,
        )

        cert_path = Path(tmpdir) / "codesign0"
        cert_der = cert_path.read_bytes()

    cert = x509.load_der_x509_certificate(cert_der)
    fingerprint = cert.fingerprint(hashes.SHA256())
    return fingerprint.hex().upper()


def parse_codesign_output(output: str) -> dict[str, str]:
    """Parse codesign -dv --verbose=4 output into a dictionary."""
    result = {}
    for line in output.splitlines():
        if "=" in line:
            key, _, value = line.partition("=")
            result[key] = value
    return result


@pytest.fixture(scope="class")
def certificate_info(ana_binary: Path | None) -> dict[str, str]:
    """Get codesign information for the binary."""
    if ana_binary is None:
        pytest.skip(
            "ana binary not found. Build with 'pixi run build-release' or set ANA_BINARY_PATH"
        )

    result = subprocess.run(
        ["codesign", "-dv", "--verbose=4", ana_binary],
        capture_output=True,
        text=True,
    )
    if result.returncode != 0:
        return {}
    return {
        **parse_codesign_output(result.stderr),
        "fingerprint": get_signing_cert_fingerprint(ana_binary),
    }


class TestMacOSSigning:
    """Tests for macOS code signing verification."""

    def test_binary_signed(
        self,
        certificate_info: dict[str, str],
    ) -> None:
        assert certificate_info, "Binary is not signed"

    def test_certificate_fingerprint(
        self,
        certificate_info: dict[str, str],
    ) -> None:
        """Verify the signing certificate matches expected fingerprint."""
        if not (
            expected_fingerprint := os.environ.get("MACOS_CERTIFICATE_FINGERPRINT")
        ):
            pytest.skip("MACOS_CERTIFICATE_FINGERPRINT not set")

        actual = certificate_info.get("fingerprint")
        expected = expected_fingerprint.upper().replace(":", "")
        assert actual == expected, (
            f"Certificate fingerprint mismatch.\n"
            f"Expected: {expected}\n"
            f"Actual:   {actual}"
        )

    def test_developer_id(
        self,
        certificate_info: dict[str, str],
    ) -> None:
        """Verify the signing identity matches expected developer ID."""
        if not (expected_developer_id := os.environ.get("MACOS_DEVELOPER_ID")):
            pytest.skip("MACOS_DEVELOPER_ID not set")

        actual = certificate_info.get("Authority", "")
        assert actual == expected_developer_id, (
            f"Developer ID mismatch.\n"
            f"Expected: {expected_developer_id}\n"
            f"Actual:   {actual}"
        )

    def test_hardened_runtime(self, certificate_info: dict[str, str]) -> None:
        """Verify the binary has hardened runtime enabled."""
        flags = certificate_info.get("CodeDirectory", "")
        assert "flags=0x10000(runtime)" in flags, (
            "Hardened runtime not enabled. "
            f"Binary must be signed with --options runtime. Flags: {flags}"
        )


def test_notarization(ana_binary: Path | None) -> None:
    """Verify notarization status matches expected state."""
    if ana_binary is None:
        pytest.skip("ana binary not found")

    expected_notarized = os.environ.get("MACOS_IS_NOTARIZED", "").lower() == "true"

    result = subprocess.run(
        ["spctl", "--assess", "--type", "execute", "-v", ana_binary],
        capture_output=True,
        text=True,
    )

    # spctl rejects standalone binaries even when notarized (they're not app bundles)
    # but the message differs:
    # - Notarized: "rejected (the code is valid but does not seem to be an app)"
    # - Not notarized: "rejected"
    is_notarized = "the code is valid" in result.stderr
    assert expected_notarized == is_notarized
