#!/usr/bin/env python3
"""Read-only release checks. Does not sign in, send mail or access any document.

Run from the matching source checkout after deploying API and web:
    python infra/demo/check-connectors.py https://app.dynodoc.online
This checks delivery/auth boundaries, not acceptance inside installed editors.
"""
import hashlib
import json
from pathlib import Path
import sys
import time
import urllib.error
import urllib.parse
import urllib.request


class NoRedirect(urllib.request.HTTPRedirectHandler):
    def redirect_request(self, req, fp, code, msg, headers, newurl):
        return None


def check(origin):
    parsed = urllib.parse.urlsplit(origin)
    if (
        parsed.scheme not in {"https", "http"}
        or not parsed.hostname
        or parsed.username
        or parsed.password
        or parsed.path not in {"", "/"}
        or parsed.query
        or parsed.fragment
        or (parsed.scheme == "http" and parsed.hostname not in {"localhost", "127.0.0.1", "::1"})
    ):
        raise ValueError("Use an HTTPS origin, or HTTP on loopback for a local test.")
    origin = origin.rstrip("/")
    root = Path(__file__).resolve().parents[2]
    static = root / "frontend/researcher/static"
    opener = urllib.request.build_opener(NoRedirect)
    results = []

    def request(path, status=200, headers=None):
        req = urllib.request.Request(origin + path, headers=headers or {})
        try:
            response = opener.open(req, timeout=20)
        except urllib.error.HTTPError as error:
            response = error
        with response:
            if response.code != status:
                raise RuntimeError(f"{path.split('?')[0]}: expected {status}, got {response.code}")
            body = response.read(2 * 1024 * 1024 + 1)
            if len(body) > 2 * 1024 * 1024:
                raise RuntimeError("Release check response exceeds 2 MiB.")
            results.append({"path": path.split("?")[0], "status": status})
            return response.headers, body

    _, body = request("/healthz")
    if json.loads(body).get("status") != "ok":
        raise RuntimeError("Web health check failed.")
    _, body = request("/auth/options")
    options = json.loads(body)
    if options.get("development") or not (options.get("email") or options.get("google")):
        raise RuntimeError("Expected normal sign-in with development authentication disabled.")
    request("/connectors")
    for host in ["word", "excel", "powerpoint", "commands.html"]:
        headers, _ = request("/connectors/" + host)
        if headers.get("x-frame-options"):
            raise RuntimeError(f"{host}: X-Frame-Options prevents Office embedding.")
        policy = headers.get("content-security-policy", "")
        if "frame-ancestors" not in policy or "https://*.officeapps.live.com" not in policy:
            raise RuntimeError(f"{host}: missing Office frame policy.")

    artifacts = [f"packages/{host}.xml" for host in ["word", "excel", "powerpoint"]]
    artifacts += [f"packages/google-{host}.zip" for host in ["docs", "sheets", "slides"]]
    artifacts += [f"assets/icon-{size}.png" for size in [16, 32, 80]]
    artifacts += ["commands.html", "commands.js"]
    for artifact in artifacts:
        path = "/connectors/" + artifact
        _, body = request(path)
        expected = (static / path.lstrip("/")).read_bytes()
        if body != expected:
            raise RuntimeError(f"{path}: deployed bytes differ from this checkout.")
        results[-1]["sha256"] = hashlib.sha256(body).hexdigest()

    for path in ["/workspace", "/projects"]:
        headers, _ = request(path, 303)
        target = urllib.parse.urlsplit(headers.get("location", ""))
        if target.path != "/login" or target.netloc or target.scheme:
            raise RuntimeError(f"{path}: expected local sign-in redirect.")

    # A public, synthetic hash; there is no secret, account or pending grant.
    approval = "/connectors/authorize?" + urllib.parse.urlencode(
        {"host": "word", "challenge": "0" * 64, "expires": int(time.time()) + 600}
    )
    headers, _ = request(approval, 303)
    target = urllib.parse.urlsplit(headers.get("location", ""))
    if (
        target.path != "/login"
        or target.netloc
        or target.scheme
        or urllib.parse.parse_qs(target.query).get("next") != [approval]
    ):
        raise RuntimeError("Approval does not preserve its local return path through sign-in.")
    request("/api/engine/documents", 401)
    request("/api/connectors/connection", 401)
    request("/api/connectors/connection", 401, {"Authorization": "Bearer " + "0" * 64})
    return {
        "origin": origin,
        "checks": results,
        "result": "passed",
        "scope": "read-only delivery and anonymous auth boundaries; not installed-host acceptance",
    }


if __name__ == "__main__":
    try:
        print(json.dumps(check(sys.argv[1] if len(sys.argv) == 2 else "https://app.dynodoc.online"), indent=2))
    except (OSError, ValueError, RuntimeError) as error:
        print(f"Connector release check failed: {error}", file=sys.stderr)
        sys.exit(1)
