"""Integration tests for macOS code signing."""

from __future__ import annotations

import os
import subprocess
from pathlib import Path

import pytest
from cryptography import x509
from cryptography.hazmat.primitives import hashes
from helpers import IS_MACOS

if not IS_MACOS:
    pytest.skip("macOS signing tests", allow_module_level=True)

# Use environment variable as sentinel variable
# since local builds are not expected to be signed
if not os.environ.get("MACOS_DEVELOPER_ID"):
    pytest.skip("Binary is not expected to be signed")


@pytest.fixture(scope="class")
def certificate_info(ana_binary: Path | None) -> list[str]:
    """Get codesign information for the binary."""
    if ana_binary is None:
        pytest.skip(
            "ana binary not found. Build with 'pixi run build-release' or set ANA_BINARY_PATH"
        )

    result = subprocess.run(
        ["codesign", "-dvvvv", ana_binary],
        capture_output=True,
        text=True,
    )
    if result.returncode != 0:
        return []
    return result.stderr.splitlines()


class TestMacOSSigning:
    """Tests for macOS code signing verification."""

    def test_binary_signed(
        self,
        certificate_info: list[str],
    ) -> None:
        assert certificate_info, "Binary is not signed"

    def test_certificate_fingerprint(
        self,
        ana_binary: Path | None,
        tmp_path: Path,
    ) -> None:
        """Verify the signing certificate matches expected fingerprint."""
        if not (
            expected_fingerprint := os.environ.get("MACOS_CERTIFICATE_FINGERPRINT")
        ):
            pytest.skip("MACOS_CERTIFICATE_FINGERPRINT not set")
        if ana_binary is None:
            pytest.skip("ana binary not found")

        # Extract public certificate
        subprocess.run(
            ["codesign", "-d", "--extract-certificates", str(ana_binary.resolve())],
            cwd=tmp_path,
            check=True,
            capture_output=True,
        )
        cert_path = tmp_path / "codesign0"
        assert cert_path.exists(), "Binary contains no certificates"

        cert_der = cert_path.read_bytes()
        cert = x509.load_der_x509_certificate(cert_der)
        fingerprint = cert.fingerprint(hashes.SHA256())
        assert fingerprint, "Fingerprint not found"
        actual_fingerprint = fingerprint.hex().upper().replace(":", "")

        assert actual_fingerprint == expected_fingerprint, (
            f"Certificate fingerprint mismatch.\n"
            f"Expected: {expected_fingerprint}\n"
            f"Actual:   {actual_fingerprint}"
        )

    def test_developer_id(
        self,
        certificate_info: list[str],
    ) -> None:
        """Verify the signing identity matches expected developer ID."""
        if not (expected_developer_id := os.environ.get("MACOS_DEVELOPER_ID")):
            pytest.skip("MACOS_DEVELOPER_ID not set")

        authorities = [
            line.split("=", 1)[1]
            for line in certificate_info
            if line.startswith("Authority=")
        ]
        assert expected_developer_id in authorities, (
            f"Developer ID mismatch.\n"
            f"Expected: {expected_developer_id}\n"
            f"Found authorities: {authorities}"
        )

    def test_hardened_runtime(self, certificate_info: list[str]) -> None:
        """Verify the binary has hardened runtime enabled."""
        code_directory = [
            line for line in certificate_info if line.startswith("CodeDirectory")
        ]
        assert len(code_directory) == 1, "Malformed CodeDirectory entry in certificate"
        flags = code_directory[0].split()
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
