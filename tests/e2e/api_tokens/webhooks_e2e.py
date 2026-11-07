"""Webhooks, end to end over HTTP: see README.md in this folder for the one-time setup.

Run: python3 webhooks_e2e.py. It starts a small receiver on this machine (RECEIVER_PORT, default 19911),
subscribes it, makes the plugin `tokplug` announce events, and checks what arrives: the signature, the
fields, retries, the log, replay, switching off, rotation, and the refusal to call private addresses.
Needs an organization administrator (AETHER_USER) and an ordinary member (AETHER_USER2). It changes the
organization's webhook settings (fast retries, plain http to this machine) and puts them back.
Exits 1 when a check fails."""
import hashlib
import hmac
import json
import os
import threading
import time
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

from e2e_lib import *

PORT = int(os.environ.get("RECEIVER_PORT", "19911"))
USER2 = os.environ.get("AETHER_USER2", "bob")
PASSWORD2 = os.environ.get("AETHER_PASSWORD2", "Test-pass-123")
URL = f"http://127.0.0.1:{PORT}/hook"

received, lock = [], threading.Lock()
mode = {"answer": 200}


class Receiver(BaseHTTPRequestHandler):
    def do_POST(self):
        body = self.rfile.read(int(self.headers.get("content-length", 0)))
        with lock:
            received.append({"headers": {k.lower(): v for k, v in self.headers.items()}, "body": body})
        self.send_response(mode["answer"])
        self.send_header("content-length", "2")
        self.end_headers()
        self.wfile.write(b"ok")

    def log_message(self, *args):
        pass


server = ThreadingHTTPServer(("127.0.0.1", PORT), Receiver)
threading.Thread(target=server.serve_forever, daemon=True).start()


def wait_for(predicate, seconds=25):
    end = time.time() + seconds
    while time.time() < end:
        if predicate():
            return True
        time.sleep(0.3)
    return False


def count():
    with lock:
        return len(received)


def verify(request, secrets):
    """What a receiver does: recompute HMAC-SHA256 of '<t>.<body>' and compare with a v1 in the header."""
    header = dict(part.split("=", 1) for part in request["headers"]["x-aether-signature"].split(",") if "v1" not in part.split("=")[0])
    t = header["t"]
    signatures = [part.split("=", 1)[1] for part in request["headers"]["x-aether-signature"].split(",") if part.startswith("v1=")]
    good = [hmac.new(secret.encode(), f"{t}.".encode() + request["body"], hashlib.sha256).hexdigest() for secret in secrets]
    return all(g in signatures for g in good), len(signatures)


def sign_in(user, password):
    b = Browser()
    b.call("GET", "/api/auth/methods")
    code, body = b.call("POST", "/api/auth/login", {"username": user, "password": password})
    check(f"{user} signs in", code == 200, (code, body))
    return b


admin = sign_in(USER, PASSWORD)
member = sign_in(USER2, PASSWORD2)


def setting(key, value):
    code, body = admin.call("PUT", f"/api/settings/{key}", {"value": value})
    check(f"setting {key}", code == 200, (code, body))


code, body = admin.call("POST", "/api/ui/tokens", {"name": f"webhooks {RUN}", "scopes": {"functions": ["tokplug.win", "tokplug.quiet"]}})
token = body.get("token")
check("a token to make the plugin announce events", code == 201, (code, body))


def announce(function="win", **fields):
    payload = {"id": f"lead:{RUN}", "title": f"Deal {RUN}", "amount": "10.50", **fields}
    code, body = machine("POST", f"/api/plugins/tokplug/{function}", token, payload)
    check(f"tokplug.{function} announces", code == 200, (code, body))


# --- who may manage them, and what is refused
code, body = member.call("GET", "/api/ui/webhooks")
check("an ordinary member may not list webhooks", code == 403, (code, body))
code, body = member.call("POST", "/api/ui/webhooks", {"name": "x", "url": URL, "events": ["tokplug.lead_won"]})
check("nor make one", code == 403, (code, body))
code, body = admin.call("POST", "/api/ui/webhooks", {"name": "plain http", "url": URL, "events": ["tokplug.lead_won"]})
check("a plain http address is refused by default", code == 422 and "https" in json.dumps(body), (code, body))

setting("webhooks.allow_http", True)
setting("webhooks.allowed_hosts", ["127.0.0.1"])
setting("webhooks.retry_base_secs", 1)
setting("webhooks.max_attempts", 3)
setting("webhooks.disable_after", 2)

for label, payload in [
    ("a name that is not an event", {"name": "x", "url": URL, "events": ["lead_won"]}),
    ("a plugin that is not installed", {"name": "x", "url": URL, "events": ["nosuch.lead_won"]}),
    ("no events", {"name": "x", "url": URL, "events": []}),
    ("no name", {"name": " ", "url": URL, "events": ["tokplug.lead_won"]}),
    ("credentials in the address", {"name": "x", "url": f"http://u:p@127.0.0.1:{PORT}/", "events": ["tokplug.lead_won"]}),
    ("a nested condition", {"name": "x", "url": URL, "events": ["tokplug.lead_won"], "conditions": {"a": {"b": 1}}}),
]:
    code, body = admin.call("POST", "/api/ui/webhooks", payload)
    check(f"refused: {label}", code == 422, (code, body))

# --- a subscription, with a condition
code, made = admin.call("POST", "/api/ui/webhooks", {"name": f"receiver {RUN}", "url": URL, "events": ["tokplug.lead_won"], "conditions": {"status": "won"}})
check("a webhook is made, and its secret shown once", code == 201 and made.get("secret", "").startswith("whsec_"), (code, made))
sub, secret = made["id"], made["secret"]
code, listing = admin.call("GET", "/api/ui/webhooks")
mine = next((s for s in listing["subscriptions"] if s["id"] == sub), None)
check("it is listed, without its secret", code == 200 and mine and "secret" not in mine and secret not in json.dumps(listing), (code, listing))

# --- a delivery
before = count()
announce()
check("the event arrives", wait_for(lambda: count() > before), "nothing received")
with lock:
    first = received[-1]
envelope = json.loads(first["body"])
ok, n = verify(first, [secret])
check("it is signed with the secret (HMAC-SHA256 of 't.body')", ok and n == 1, first["headers"])
check("headers name the event and the delivery", first["headers"].get("x-aether-event") == "tokplug.lead_won" and first["headers"].get("x-aether-delivery") == envelope["id"] and first["headers"].get("x-aether-attempt") == "1", first["headers"])
check("the envelope has id, event, created_at and data", set(envelope) == {"id", "event", "created_at", "data"} and envelope["event"] == "tokplug.lead_won", envelope)
check("only the allowed fields leave", envelope["data"] == {"id": f"lead:{RUN}", "title": f"Deal {RUN}", "amount": "10.50", "status": "won"} and "internal" not in first["body"].decode(), envelope)
check("the signature's time is recent", abs(int(first["headers"]["x-aether-signature"].split(",")[0][2:]) - time.time()) < 60)

check("the log says delivered", wait_for(lambda: any(d["ulid"] == envelope["id"] and d["status"] == "delivered" for d in admin.call("GET", f"/api/ui/webhooks/{sub}/deliveries")[1]["deliveries"])), "not delivered in the log")
log = next(d for d in admin.call("GET", f"/api/ui/webhooks/{sub}/deliveries")[1]["deliveries"] if d["ulid"] == envelope["id"])
check("with the receiver's status and a duration", log["http_status"] == 200 and log["attempts"] == 1 and log["duration_ms"] is not None and log["response_head"] == "ok", log)

# --- what is not sent
before = count()
announce(status="lost")
announce("quiet")
time.sleep(3)
check("an event that fails the condition is not sent", count() == before, count() - before)

# --- retries, giving up, replay
mode["answer"] = 500
before = count()
announce(title=f"Failing {RUN}")
check("a failing receiver is tried again (3 attempts)", wait_for(lambda: count() - before >= 3), count() - before)
with lock:
    attempts = [r["headers"]["x-aether-attempt"] for r in received[before:]]
check("each try says which attempt it is", attempts[:3] == ["1", "2", "3"], attempts)
check("after the last the delivery is dead", wait_for(lambda: any(d["status"] == "dead" for d in admin.call("GET", f"/api/ui/webhooks/{sub}/deliveries")[1]["deliveries"])), "not dead")
dead = next(d for d in admin.call("GET", f"/api/ui/webhooks/{sub}/deliveries")[1]["deliveries"] if d["status"] == "dead")
check("with the reason", dead["http_status"] == 500 and "500" in (dead["error"] or ""), dead)
mode["answer"] = 200
before = count()
code, body = admin.call("POST", f"/api/ui/webhooks/deliveries/{dead['ulid']}/replay")
check("a given-up delivery is sent again by hand", code == 200 and body.get("replay_of") == dead["ulid"], (code, body))
check("and arrives", wait_for(lambda: count() > before), "nothing received")
with lock:
    replayed = json.loads(received[-1]["body"])
check("as a new delivery with the same data", replayed["id"] != dead["ulid"] and replayed["data"]["title"] == f"Failing {RUN}", replayed)
code, body = admin.call("POST", "/api/ui/webhooks/deliveries/nosuchdelivery/replay")
check("an unknown delivery is a 404", code == 404, (code, body))

# --- switching off
mode["answer"] = 500
for n in range(2):
    before = count()
    announce(title=f"Down {n} {RUN}")
    wait_for(lambda: count() - before >= 3)
check("deliveries that keep giving up switch the webhook off", wait_for(lambda: next(s for s in admin.call("GET", "/api/ui/webhooks")[1]["subscriptions"] if s["id"] == sub)["active"] is False), "still on")
off = next(s for s in admin.call("GET", "/api/ui/webhooks")[1]["subscriptions"] if s["id"] == sub)
check("and says why", "in a row" in (off.get("disabled_reason") or ""), off)
mode["answer"] = 200
before = count()
announce(title=f"While off {RUN}")
time.sleep(3)
check("nothing is sent while it is off", count() == before)
code, body = admin.call("POST", f"/api/ui/webhooks/deliveries/{dead['ulid']}/replay")
check("and a delivery cannot be replayed into it", code == 409, (code, body))
code, body = admin.call("PATCH", f"/api/ui/webhooks/{sub}", {"active": True})
check("an administrator switches it on again", code == 200, (code, body))
before = count()
announce(title=f"Back {RUN}")
check("and events flow again", wait_for(lambda: count() > before), "nothing received")

# --- rotating the secret
code, rotated = admin.call("POST", f"/api/ui/webhooks/{sub}/rotate")
new_secret = rotated.get("secret")
check("the secret is rotated, the new one shown once", code == 200 and new_secret and new_secret != secret, (code, rotated))
before = count()
announce(title=f"Rotated {RUN}")
check("the next event arrives", wait_for(lambda: count() > before), "nothing received")
with lock:
    rotated_request = received[-1]
ok, n = verify(rotated_request, [new_secret, secret])
check("it is signed by the new secret and, for a day, the old one too", ok and n == 2, rotated_request["headers"])
code, listing = admin.call("GET", "/api/ui/webhooks")
check("the list says the old secret still signs", next(s for s in listing["subscriptions"] if s["id"] == sub)["old_secret_still_signs"] is True, listing)

# --- a test event
before = count()
code, body = admin.call("POST", f"/api/ui/webhooks/{sub}/test")
check("a test ping is queued", code == 200, (code, body))
check("and arrives, signed", wait_for(lambda: count() > before), "nothing received")
with lock:
    ping = received[-1]
check("as the aether.ping event", ping["headers"]["x-aether-event"] == "aether.ping" and json.loads(ping["body"])["data"]["ping"] is True, ping["headers"])

# --- private addresses are refused unless the host is listed
setting("webhooks.allowed_hosts", [])
before = count()
announce(title=f"Blocked {RUN}")
check("a private address is refused (nothing reaches the receiver)", wait_for(lambda: any("private" in (d.get("error") or "") for d in admin.call("GET", f"/api/ui/webhooks/{sub}/deliveries")[1]["deliveries"]), 30) and count() == before, count() - before)
setting("webhooks.allowed_hosts", ["127.0.0.1"])

# --- tidy up
code, body = admin.call("DELETE", f"/api/ui/webhooks/{sub}")
check("the webhook is deleted", code == 200, (code, body))
code, body = admin.call("DELETE", f"/api/ui/webhooks/{sub}")
check("twice is a 404", code == 404, (code, body))
for key, value in [("webhooks.allow_http", False), ("webhooks.allowed_hosts", []), ("webhooks.retry_base_secs", 60), ("webhooks.max_attempts", 8), ("webhooks.disable_after", 20)]:
    setting(key, value)

finish()
