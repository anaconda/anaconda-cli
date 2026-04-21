"""Mock authentication server for integration testing.

This server mimics the Anaconda authentication endpoints:
- /.well-known/openid-configuration
- /oauth/device/code (device authorization)
- /oauth/token (token exchange)
- /api/account (user info)
- /api/auth/api-keys (API key creation)
- /api/auth/sessions/whoami (user info with organizations)
"""

from __future__ import annotations

import base64
import json
import threading
import time
from http.server import BaseHTTPRequestHandler
from http.server import HTTPServer
from typing import Any
from urllib.parse import parse_qs

# Mock data
MOCK_USER = {
    "user": {
        "id": "test-user-id",
        "username": "testuser",
        "email": "test@example.com",
        "first_name": "Test",
        "last_name": "User",
    },
    "profile": {
        "is_confirmed": True,
        "is_consented": True,
        "is_disabled": False,
    },
    "subscriptions": [],
}

# Whoami response (used by the new styled output)
MOCK_WHOAMI = {
    "identity": {"id": "test-user-id"},
    "passport": {
        "user_id": "test-user-id",
        "profile": {
            "email": "test@example.com",
            "first_name": "Test",
            "last_name": "User",
            "username": "testuser",
        },
        "organizations": [
            {
                "org_id": "test-org-id",
                "name": "test-org",
                "title": "Test Organization",
                "role": "member",
                "attributes": [
                    {
                        "id": "test_subscription",
                        "group": "subscriptions",
                        "data": {"expires_at": "2030-01-01 00:00:00+00:00"},
                    }
                ],
            }
        ],
    },
}


def _create_mock_jwt() -> str:
    """Create a mock JWT with an expiration claim (1 year from now)."""
    # JWT header
    header = {"alg": "none", "typ": "JWT"}
    # JWT payload with exp claim (1 year from now)
    exp_timestamp = int(time.time()) + 365 * 24 * 60 * 60
    payload = {
        "exp": exp_timestamp,
        "sub": "test-user-id",
        "scopes": ["cloud:read", "cloud:write", "repo:read"],
    }
    # Encode as base64url (no padding)
    header_b64 = (
        base64.urlsafe_b64encode(json.dumps(header).encode()).rstrip(b"=").decode()
    )
    payload_b64 = (
        base64.urlsafe_b64encode(json.dumps(payload).encode()).rstrip(b"=").decode()
    )
    # Return JWT (no signature for mock)
    return f"{header_b64}.{payload_b64}."


MOCK_API_KEY = _create_mock_jwt()
MOCK_ACCESS_TOKEN = "mock-access-token-67890"
MOCK_DEVICE_CODE = "mock-device-code"
MOCK_USER_CODE = "TEST-1234"


class MockAuthHandler(BaseHTTPRequestHandler):
    """HTTP request handler for mock auth endpoints."""

    # Class-level state for device flow
    device_authorized: dict[str, bool] = {}
    auto_approve_delay: float = 0.5  # seconds

    def log_message(self, format: str, *args: Any) -> None:
        """Suppress logging unless DEBUG is set."""
        pass

    def _send_json(self, data: dict[str, Any], status: int = 200) -> None:
        """Send a JSON response."""
        self.send_response(status)
        self.send_header("Content-Type", "application/json")
        self.end_headers()
        self.wfile.write(json.dumps(data).encode())

    def _send_error_json(self, error: str, description: str, status: int = 400) -> None:
        """Send a JSON error response."""
        self._send_json({"error": error, "error_description": description}, status)

    def _get_auth_header(self) -> str | None:
        """Extract bearer token from Authorization header."""
        auth = self.headers.get("Authorization", "")
        if auth.startswith("Bearer "):
            return auth[7:]
        return None

    def do_GET(self) -> None:
        """Handle GET requests."""
        if self.path == "/.well-known/openid-configuration":
            self._handle_openid_config()
        elif self.path == "/api/account":
            self._handle_account()
        elif self.path == "/api/auth/sessions/whoami":
            self._handle_whoami()
        else:
            self.send_error(404)

    def do_POST(self) -> None:
        """Handle POST requests."""
        content_length = int(self.headers.get("Content-Length", 0))
        body = self.rfile.read(content_length).decode()

        if self.path == "/oauth/device/code":
            self._handle_device_auth(body)
        elif self.path == "/oauth/token":
            self._handle_token(body)
        elif self.path == "/api/auth/api-keys":
            self._handle_create_api_key()
        else:
            self.send_error(404)

    def _handle_openid_config(self) -> None:
        """Return OpenID configuration."""
        host = self.headers.get("Host", "localhost")
        base_url = f"http://{host}"
        self._send_json(
            {
                "issuer": base_url,
                "device_authorization_endpoint": f"{base_url}/oauth/device/code",
                "token_endpoint": f"{base_url}/oauth/token",
            }
        )

    def _handle_device_auth(self, body: str) -> None:
        """Handle device authorization request."""
        params = parse_qs(body)
        client_id = params.get("client_id", [None])[0]

        if not client_id:
            self._send_error_json("invalid_request", "client_id is required")
            return

        # Generate device code and mark as pending
        device_code = f"{MOCK_DEVICE_CODE}-{time.time()}"
        MockAuthHandler.device_authorized[device_code] = False

        # Auto-approve after delay (simulates user completing flow)
        def auto_approve() -> None:
            time.sleep(MockAuthHandler.auto_approve_delay)
            MockAuthHandler.device_authorized[device_code] = True

        threading.Thread(target=auto_approve, daemon=True).start()

        host = self.headers.get("Host", "localhost")
        self._send_json(
            {
                "device_code": device_code,
                "user_code": MOCK_USER_CODE,
                "verification_uri": f"http://{host}/verify",
                "verification_uri_complete": f"http://{host}/verify?code={MOCK_USER_CODE}",
                "expires_in": 300,
                "interval": 1,
            }
        )

    def _handle_token(self, body: str) -> None:
        """Handle token request (polling)."""
        params = parse_qs(body)
        grant_type = params.get("grant_type", [None])[0]
        device_code = params.get("device_code", [None])[0]

        if grant_type != "urn:ietf:params:oauth:grant-type:device_code":
            self._send_error_json("unsupported_grant_type", "Invalid grant type")
            return

        if not device_code:
            self._send_error_json("invalid_request", "device_code is required")
            return

        # Check if device is authorized
        if device_code not in MockAuthHandler.device_authorized:
            self._send_error_json("invalid_grant", "Unknown device code")
            return

        if not MockAuthHandler.device_authorized[device_code]:
            self._send_error_json(
                "authorization_pending", "User has not yet authorized"
            )
            return

        # Device is authorized, return token
        self._send_json(
            {
                "access_token": MOCK_ACCESS_TOKEN,
                "token_type": "Bearer",
                "expires_in": 3600,
            }
        )

    def _handle_account(self) -> None:
        """Handle account info request."""
        token = self._get_auth_header()
        if not token:
            self._send_error_json("unauthorized", "Missing authorization", 401)
            return

        self._send_json(MOCK_USER)

    def _handle_whoami(self) -> None:
        """Handle whoami request (new styled output endpoint)."""
        token = self._get_auth_header()
        if not token:
            self._send_error_json("unauthorized", "Missing authorization", 401)
            return

        self._send_json(MOCK_WHOAMI)

    def _handle_create_api_key(self) -> None:
        """Handle API key creation."""
        token = self._get_auth_header()
        if not token:
            self._send_error_json("unauthorized", "Missing authorization", 401)
            return

        self._send_json({"api_key": MOCK_API_KEY}, status=201)


class MockAuthServer:
    """Context manager for running the mock auth server."""

    def __init__(self, host: str = "127.0.0.1", port: int = 0) -> None:
        self.host = host
        self.port = port
        self.server: HTTPServer | None = None
        self.thread: threading.Thread | None = None

    def __enter__(self) -> MockAuthServer:
        self.server = HTTPServer((self.host, self.port), MockAuthHandler)
        # Get the actual port if 0 was specified
        self.port = self.server.server_address[1]
        self.thread = threading.Thread(target=self.server.serve_forever, daemon=True)
        self.thread.start()
        return self

    def __exit__(self, *args: Any) -> None:
        if self.server:
            self.server.shutdown()

    @property
    def base_url(self) -> str:
        return f"http://{self.host}:{self.port}"

    @property
    def domain(self) -> str:
        return f"{self.host}:{self.port}"
