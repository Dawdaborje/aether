"""Inbound hooks, end to end over HTTP: see README.md in this folder for the one-time setup.

Run: python3 inbound_e2e.py. It plays the outside system: it calls the addresses of the plugin `tokplug`
the way a payment provider would (a signature or a key in a header, no cookies, no CSRF token) and checks
what is let in and what is not. Needs an organization administrator (AETHER_USER) and an ordinary member
(AETHER_USER2). Exits 1 when a check fails."""
import hashlib
import hmac
import json
import os
import time
import urllib.error
import urllib.request

from e2e_lib import *

USER2 = os.environ.get("AETHER_USER2", "bob")
PASSWORD2 = os.environ.get("AETHER_PASSWORD2", "Test-pass-123")
ORG = os.environ.get("AETHER_ORG", "tok_org")


def post(path, body=b"", headers=None, method="POST"):
    request = urllib.request.Request(BASE + path, data=body if method != "GET" else None, method=method, headers=headers or {})
    try:
        with urllib.request.urlopen(request) as response:
            raw = response.read()
            return response.status, (json.loads(raw) if raw else None)
    except urllib.error.HTTPError as error:
        raw = error.read()
        try:
            return error.code, json.loads(raw)
        except Exception:
            return error.code, raw.decode(errors="replace")


def sign(secret, body):
    return hmac.new(secret.encode(), body, hashlib.sha256).hexdigest()


def aether_header(secret, body, t=None):
    t = int(time.time()) if t is None else t
    return f"t={t},v1=" + hmac.new(secret.encode(), f"{t}.".encode() + body, hashlib.sha256).hexdigest()


def sign_in(user, password):
    b = Browser()
    b.call("GET", "/api/auth/methods")
    code, body = b.call("POST", "/api/auth/login", {"username": user, "password": password})
    check(f"{user} signs in", code == 200, (code, body))
    return b


admin = sign_in(USER, PASSWORD)
member = sign_in(USER2, PASSWORD2)
URL = lambda name: f"/api/hooks/{ORG}/tokplug/{name}"

# Start from no secrets, so the script can be run again.
for leftover in ["payments", "key", "signed", "broken"]:
    admin.call("DELETE", f"/api/ui/inbound/tokplug/{leftover}")

# --- the administrator sees the plugin's hooks, none usable yet
code, described = admin.call("GET", "/api/ui/inbound/tokplug")
hooks = {h["name"]: h for h in described.get("hooks", [])}
check("the plugin's four hooks are listed", code == 200 and set(hooks) == {"payments", "key", "signed", "broken"}, (code, described))
check("each says how it is verified and where it lives", hooks["payments"]["verify"] == "hmac-sha256" and hooks["payments"]["header"] == "x-signature" and hooks["payments"]["address"] == URL("payments"), hooks["payments"])
check("none has a secret yet", not any(h["has_secret"] for h in hooks.values()), hooks)
body = b'{"amount": 5}'
code, answer = post(URL("payments"), body, {"x-signature": sign("whsec_anything", body)})
check("a hook without a secret answers 404", code == 404 and answer == {"error": "not found"}, (code, answer))
code, again = post(URL("nosuchhook"), body, {})
check("and says the same as one that does not exist (nothing to probe)", code == 404 and again == answer, (code, again))
code, again = post(f"/api/hooks/no_such_org/tokplug/payments", body, {})
check("or an organization that does not exist", code == 404 and again == answer, (code, again))
code, again = post(f"/api/hooks/{ORG}/nosuchplugin/payments", body, {})
check("or a plugin that does not exist", code == 404 and again == answer, (code, again))

# --- who may make secrets
code, body_ = member.call("POST", "/api/ui/inbound/tokplug/payments/secret")
check("an ordinary member may not make a secret", code == 403, (code, body_))
code, body_ = admin.call("POST", "/api/ui/inbound/tokplug/nosuch/secret")
check("a hook the plugin does not declare gets no secret", code == 404, (code, body_))
code, body_ = admin.call("POST", "/api/ui/inbound/noplugin/x/secret")
check("nor does a plugin that is not installed", code == 404, (code, body_))

secrets = {}
for name in ["payments", "key", "signed", "broken"]:
    code, made = admin.call("POST", f"/api/ui/inbound/tokplug/{name}/secret")
    check(f"{name}: a secret is made and shown once", code == 201 and made.get("secret", "").startswith("whsec_") and made["address"] == URL(name), (code, made))
    secrets[name] = made.get("secret")

# --- hmac-sha256 (the form many services use)
payload = json.dumps({"amount": 125}).encode()
code, answer = post(URL("payments") + "?n=7", payload, {"content-type": "application/json", "x-signature": sign(secrets["payments"], payload), "x-event": "paid", "cookie": "aether_session=whatever"})
check("a correctly signed call reaches the plugin", code == 200 and answer["received"] is True and answer["amount"] == 125 and answer["hook"] == "payments", (code, answer))
check("the plugin sees the body, the query and ordinary headers", answer["event"] == "paid" and answer["n"] == "7" and answer["raw_length"] == len(payload), answer)
check("but never the signature or a cookie", answer["saw_signature"] is False and answer["saw_cookie"] is False, answer)
code, answer = post(URL("payments"), payload, {"content-type": "application/json", "x-signature": "sha256=" + sign(secrets["payments"], payload)})
check("a 'sha256=' prefix is accepted", code == 200, (code, answer))
code, answer = post(URL("payments"), payload, {"x-signature": sign(secrets["payments"], payload + b" ")})
check("a signature of another body is refused (401)", code == 401 and answer == {"error": "invalid signature"}, (code, answer))
code, answer = post(URL("payments"), payload, {"x-signature": sign("whsec_wrong", payload)})
check("a signature under another secret is refused", code == 401, (code, answer))
code, answer = post(URL("payments"), payload, {})
check("no signature is refused", code == 401, (code, answer))
code, answer = post(URL("payments"), payload, {"x-signature": "not hex"})
check("a malformed one is refused", code == 401, (code, answer))
code, answer = admin.call("POST", URL("payments"), json.loads(payload), csrf=True)
check("a signed-in browser without the signature is refused too (cookies prove nothing here)", code == 401, (code, answer))
code, answer = post(URL("payments"), payload, {"x-signature": sign(secrets["payments"], payload)}, method="GET")
check("only POST is accepted", code == 405, (code, answer))

# --- token
code, answer = post(URL("key"), b"hello", {"x-api-key": secrets["key"]})
check("a key in the header is accepted (the body need not be JSON)", code == 200 and answer["raw"] == "hello", (code, answer))
code, answer = post(URL("key"), b"hello", {"x-api-key": secrets["key"] + "x"})
check("a wrong key is refused", code == 401, (code, answer))
code, answer = post(URL("key"), b"hello", {"x-api-key": secrets["payments"]})
check("another hook's secret is not accepted", code == 401, (code, answer))

# --- our own scheme: signed time, no replay
payload = b'{"hello": "world"}'
code, answer = post(URL("signed"), payload, {"x-aether-signature": aether_header(secrets["signed"], payload), "content-type": "application/json"})
check("an Aether-signed call is accepted", code == 200 and answer["body"] == {"hello": "world"}, (code, answer))
old = aether_header(secrets["signed"], payload, t=int(time.time()) - 1000)
code, answer = post(URL("signed"), payload, {"x-aether-signature": old})
check("the same call signed a while ago is refused (no replay)", code == 401, (code, answer))
code, answer = post(URL("signed"), payload, {"x-aether-signature": aether_header(secrets["signed"], payload).replace("t=", "t=1")})
check("changing the time breaks the signature", code == 401, (code, answer))

# --- the counters, rotation, switching off
code, described = admin.call("GET", "/api/ui/inbound/tokplug")
hooks = {h["name"]: h for h in described["hooks"]}
check("the administrator sees what was received and refused", hooks["payments"]["received"] == 2 and hooks["payments"]["rejected"] >= 4 and "signature" in (hooks["payments"]["last_rejected_reason"] or ""), hooks["payments"])
check("and that hooks now have secrets", all(h["has_secret"] and h["enabled"] for h in hooks.values()), hooks)

code, rotated = admin.call("POST", "/api/ui/inbound/tokplug/key/secret")
check("a secret is rotated (200, the new one shown once)", code == 200 and rotated["rotated"] is True and rotated["secret"] != secrets["key"], (code, rotated))
code, answer = post(URL("key"), b"x", {"x-api-key": rotated["secret"]})
check("the new secret works", code == 200, (code, answer))
code, answer = post(URL("key"), b"x", {"x-api-key": secrets["key"]})
check("and the old one still does, for a while, so the other system can switch", code == 200, (code, answer))

code, body_ = admin.call("PATCH", "/api/ui/inbound/tokplug/key", {"enabled": False})
check("a hook is switched off", code == 200, (code, body_))
code, answer = post(URL("key"), b"x", {"x-api-key": rotated["secret"]})
check("and answers 404 while off", code == 404, (code, answer))
code, body_ = admin.call("PATCH", "/api/ui/inbound/tokplug/key", {"enabled": True})
code, answer = post(URL("key"), b"x", {"x-api-key": rotated["secret"]})
check("and works again when switched on", code == 200, (code, answer))
code, body_ = admin.call("PATCH", "/api/ui/inbound/tokplug/nosecret", {"enabled": True})
check("a hook without a secret cannot be switched", code == 404, (code, body_))

# --- limits and failures
big = b"x" * (1024 * 1024 + 1)
code, answer = post(URL("key"), big, {"x-api-key": rotated["secret"]})
check("a body over 1 MiB is refused (413)", code == 413, (code, answer))
code, answer = post(URL("broken"), b"x", {"x-api-key": secrets["broken"]})
check("a plugin that fails answers 500 without its own words", code == 500 and "boom" not in json.dumps(answer), (code, answer))

# --- removing
code, body_ = admin.call("DELETE", "/api/ui/inbound/tokplug/payments")
check("a hook's secret is removed", code == 200, (code, body_))
code, answer = post(URL("payments"), payload, {"x-signature": sign(secrets["payments"], payload)})
check("and the address is dead again", code == 404, (code, answer))
code, body_ = admin.call("DELETE", "/api/ui/inbound/tokplug/payments")
check("removing twice is a 404", code == 404, (code, body_))

finish()
