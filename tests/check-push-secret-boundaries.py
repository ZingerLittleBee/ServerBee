"""Run native macOS URLSession redirect checks and static entitlement checks.

Compile only the Foundation transport and a synthetic client, never the iOS app.
The temporary TLS certificate and all transmitted secrets are test fixtures.
"""

import argparse
import http.server
import json
from pathlib import Path
import plistlib
import re
import socketserver
import ssl
import subprocess
import tempfile
import threading
import time


ROOT = Path(__file__).resolve().parents[1]
IOS = ROOT / "apps/ios"


def check_entitlements():
    bundle_id = "app.serverbee"
    project = (IOS / "project.yml").read_text()
    assert re.findall(r"PRODUCT_BUNDLE_IDENTIFIER: (\S+)", project) == [
        bundle_id, bundle_id + ".notifications", bundle_id + ".tests"
    ]
    worker = (ROOT / "apps/push-relay/wrangler.jsonc").read_text()
    assert re.search(r'"APNS_TOPIC":\s*"([^"\n]+)"', worker)[1] == bundle_id
    assert f'BUNDLE_ID="{bundle_id}"' in (ROOT / "scripts/ios-install.sh").read_text()
    private = "$(AppIdentifierPrefix)" + bundle_id
    shared = private + ".push"
    for config in ["Debug", "Release"]:
        with (IOS / f"ServerBee/ServerBee.{config}.entitlements").open("rb") as file:
            assert plistlib.load(file)["keychain-access-groups"] == [private, shared]
    with (IOS / "NotificationService/NotificationService.entitlements").open("rb") as file:
        assert plistlib.load(file)["keychain-access-groups"] == [shared]
    with (IOS / "ServerBee/Info.plist").open("rb") as file:
        app = plistlib.load(file)
    with (IOS / "NotificationService/Info.plist").open("rb") as file:
        extension = plistlib.load(file)
    assert app["PrivateKeychainAccessGroup"] == private
    assert app["PushKeychainAccessGroup"] == extension["PushKeychainAccessGroup"] == shared
    assert "PrivateKeychainAccessGroup" not in extension
    print("App identity and Keychain group configuration: PASS (static only, not signed entitlement proof)")


def check_redirects():
    captures = []
    targets = []

    class Target(socketserver.BaseRequestHandler):
        def handle(self):
            targets.append(self.client_address)
            self.request.close()

    class Origin(http.server.BaseHTTPRequestHandler):
        protocol_version = "HTTP/1.1"

        def do_POST(self):
            body = self.rfile.read(int(self.headers.get("Content-Length", "0")))
            captures.append((self.path, self.headers.get("Authorization"), json.loads(body)))
            parts = self.path.strip("/").split("/")
            if len(parts) != 2 or parts[0] not in ["307", "308"]:
                targets.append(self.path)
                self.send_response(200)
            else:
                status, scheme = parts
                self.send_response(int(status))
                location = (
                    f"https://localhost:{origin.server_port}/target"
                    if scheme == "same-origin"
                    else f"{scheme}://127.0.0.1:{target.server_address[1]}/target"
                )
                self.send_header("Location", location)
            self.send_header("Content-Length", "0")
            self.end_headers()

        def log_message(self, *_args):
            pass

    with tempfile.TemporaryDirectory(prefix="serverbee-redirect-") as directory:
        work = Path(directory)
        subprocess.run([
            "openssl", "req", "-x509", "-newkey", "rsa:2048", "-nodes",
            "-keyout", str(work / "fixture.key"), "-out", str(work / "fixture.crt"),
            "-days", "1", "-subj", "/CN=localhost",
        ], check=True, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        subprocess.run([
            "swiftc", "-swift-version", "6", "-module-cache-path", str(work / "swift-cache"),
            str(IOS / "ServerBee/Services/ServerHTTPTransport.swift"),
            str(ROOT / "tests/fixtures/push-redirect-client.swift"), "-o", str(work / "client"),
        ], check=True)
        print("Production Foundation transport + fixture client: Swift 6 compile PASS", flush=True)
        target = socketserver.ThreadingTCPServer(("127.0.0.1", 0), Target)
        origin = http.server.ThreadingHTTPServer(("127.0.0.1", 0), Origin)
        tls = ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER)
        tls.load_cert_chain(work / "fixture.crt", work / "fixture.key")
        origin.socket = tls.wrap_socket(origin.socket, server_side=True)
        for server in [target, origin]:
            threading.Thread(target=server.serve_forever, daemon=True).start()
        try:
            subprocess.run([str(work / "client"), f"https://localhost:{origin.server_port}"], check=True, timeout=45)
            time.sleep(0.1)
            assert len(captures) == 6, len(captures)
            assert not targets, f"Redirect targets were contacted: {targets}"
            for _path, authorization, body in captures:
                assert authorization == "Bearer synthetic-access"
                assert body["content_key"] == "synthetic-content-key"
                assert body["refresh_token"] == "synthetic-refresh"
                assert body["revocation_token"] == "synthetic-revocation"
            print("HTTPS origin received 6 secret-bearing requests; redirect targets received 0 connections")
        finally:
            for server in [origin, target]:
                server.shutdown()
                server.server_close()


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--static-only", action="store_true", help="Check source identities/groups without Swift or macOS")
    args = parser.parse_args()
    check_entitlements()
    if not args.static_only:
        check_redirects()
