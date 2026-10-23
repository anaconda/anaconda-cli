"""Linux certificate verification with a custom CA store."""

from __future__ import annotations

import http.server
import ssl
import threading
from datetime import UTC
from datetime import datetime
from datetime import timedelta
from functools import partial
from pathlib import Path

import pytest
from cryptography import x509
from cryptography.hazmat.primitives import hashes
from cryptography.hazmat.primitives import serialization
from cryptography.hazmat.primitives.asymmetric import ec
from cryptography.x509.oid import NameOID
from helpers import IS_LINUX
from helpers import AnaRunner

pytestmark = pytest.mark.skipif(not IS_LINUX, reason="Linux CA store selection")


@pytest.fixture
def tls_server(tmp_path: Path):
    """Serve HTTPS using a private CA and a localhost certificate."""
    now = datetime.now(UTC)
    root_key = ec.generate_private_key(ec.SECP256R1())
    root_name = x509.Name([x509.NameAttribute(NameOID.COMMON_NAME, "ana test CA")])
    root = (
        x509.CertificateBuilder()
        .subject_name(root_name)
        .issuer_name(root_name)
        .public_key(root_key.public_key())
        .serial_number(x509.random_serial_number())
        .not_valid_before(now - timedelta(days=1))
        .not_valid_after(now + timedelta(days=2))
        .add_extension(x509.BasicConstraints(ca=True, path_length=0), critical=True)
        .sign(root_key, hashes.SHA256())
    )
    key = ec.generate_private_key(ec.SECP256R1())
    certificate = (
        x509.CertificateBuilder()
        .subject_name(x509.Name([x509.NameAttribute(NameOID.COMMON_NAME, "localhost")]))
        .issuer_name(root_name)
        .public_key(key.public_key())
        .serial_number(x509.random_serial_number())
        .not_valid_before(now - timedelta(days=1))
        .not_valid_after(now + timedelta(days=2))
        .add_extension(x509.SubjectAlternativeName([x509.DNSName("localhost")]), False)
        .sign(root_key, hashes.SHA256())
    )
    ca_file = tmp_path / "ca.pem"
    ca_file.write_bytes(root.public_bytes(serialization.Encoding.PEM))
    cert_file = tmp_path / "server.pem"
    cert_file.write_bytes(certificate.public_bytes(serialization.Encoding.PEM))
    key_file = tmp_path / "key.pem"
    key_file.write_bytes(
        key.private_bytes(
            serialization.Encoding.PEM,
            serialization.PrivateFormat.PKCS8,
            serialization.NoEncryption(),
        )
    )
    (tmp_path / "index.html").write_text("trusted private CA\n")
    unrelated_key = ec.generate_private_key(ec.SECP256R1())
    unrelated_root = (
        x509.CertificateBuilder()
        .subject_name(root_name)
        .issuer_name(root_name)
        .public_key(unrelated_key.public_key())
        .serial_number(x509.random_serial_number())
        .not_valid_before(now - timedelta(days=1))
        .not_valid_after(now + timedelta(days=2))
        .add_extension(x509.BasicConstraints(ca=True, path_length=0), critical=True)
        .sign(unrelated_key, hashes.SHA256())
    )
    unrelated_ca = tmp_path / "unrelated.pem"
    unrelated_ca.write_bytes(unrelated_root.public_bytes(serialization.Encoding.PEM))
    ca_dir = tmp_path / "certs"
    ca_dir.mkdir()
    context = ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER)
    context.load_cert_chain(cert_file, key_file)
    handler = partial(http.server.SimpleHTTPRequestHandler, directory=str(tmp_path))
    with http.server.ThreadingHTTPServer(("127.0.0.1", 0), handler) as server:
        server.socket = context.wrap_socket(server.socket, server_side=True)
        thread = threading.Thread(target=server.serve_forever, daemon=True)
        thread.start()
        try:
            yield server.server_port, ca_file, unrelated_ca, ca_dir
        finally:
            server.shutdown()
            thread.join()


@pytest.mark.parametrize(
    ("trusted", "hostname", "expected_code"),
    [(True, "localhost", 0), (False, "localhost", 1), (True, "127.0.0.1", 1)],
)
def test_custom_ca_verification(
    run_ana: AnaRunner,
    logged_in_env: dict[str, str],
    tls_server,
    trusted: bool,
    hostname: str,
    expected_code: int,
) -> None:
    port, ca_file, unrelated_ca, ca_dir = tls_server
    result = run_ana(
        "api",
        "fetch",
        f"https://{hostname}:{port}/",
        env={
            **logged_in_env,
            "ANA_ENABLE_TELEMETRY": "false",
            "ANA_UPDATE_CHECK": "false",
            "SSL_CERT_FILE": str(ca_file if trusted else unrelated_ca),
            "SSL_CERT_DIR": str(ca_dir),
            "NO_PROXY": "localhost,127.0.0.1",
        },
    )
    assert result.returncode == expected_code, result.stderr
    if expected_code == 0:
        assert "trusted private CA" in result.stdout
    else:
        assert "error sending request" in result.stderr
