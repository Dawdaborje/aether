"""Inbound mail, end to end over HTTP: see README.md in this folder for the one-time setup.

Plays the mail provider that posts each received message to /api/mail/<org>/<provider>, and an administrator
who manages the intake. Uses the plugin `tokplug` (routes `support` and `secure`, model `contact` with chatter).
Exits 1 when a check fails."""
import base64
import json
import os
import time
import urllib.error
import urllib.request

from e2e_lib import *

ORG = os.environ.get("AETHER_ORG", "tok_org")
USER2 = os.environ.get("AETHER_USER2", "bob")
PASSWORD2 = os.environ.get("AETHER_PASSWORD2", "Test-pass-123")
P = "/api/plugins/tokplug/"
DOMAIN = "mail.acme.test"


def sign_in(user, password):
    b = Browser()
    b.call("GET", "/api/auth/methods")
    code, body = b.call("POST", "/api/auth/login", {"username": user, "password": password})
    check(f"{user} signs in", code == 200, (code, body))
    return b


def post_mail(provider, body, secret=None, basic=False, org=ORG, raw=None):
    headers = {"Content-Type": "application/json"}
    if secret and basic:
        headers["Authorization"] = "Basic " + base64.b64encode(f"mail:{secret}".encode()).decode()
    elif secret:
        headers["X-Aether-Mail-Token"] = secret
    data = raw if raw is not None else json.dumps(body).encode()
    request = urllib.request.Request(f"{BASE}/api/mail/{org}/{provider}", data=data, method="POST", headers=headers)
    try:
        with urllib.request.urlopen(request) as response:
            return response.status, json.loads(response.read() or b"null")
    except urllib.error.HTTPError as error:
        raw_body = error.read()
        try:
            return error.code, json.loads(raw_body)
        except Exception:
            return error.code, raw_body.decode(errors="replace")


counter = [0]


def mail(to, subject="Hello", sender="ann@client.test", text="the text", **extra):
    counter[0] += 1
    body = {"from": {"name": "Ann", "address": sender}, "to": [to] if isinstance(to, str) else to, "subject": subject,
            "message_id": f"<m{counter[0]}.{RUN}@client.test>", "text": text, "received_at": "2026-10-18T10:00:00Z"}
    body.update(extra)
    return body


def memos(admin):
    return [m["body"] for m in admin.call("POST", P + "memos", {})[1]]


def wait_memo(admin, needle, seconds=30):
    end = time.time() + seconds
    while time.time() < end:
        found = [m for m in memos(admin) if needle in m]
        if found:
            return found
        time.sleep(0.5)
    return []


def setting(admin, key, value):
    code, body = admin.call("PUT", f"/api/settings/{key}", {"value": value})
    check(f"setting {key}", code in (200, 204), (code, body))


admin = sign_in(USER, PASSWORD)
bob = sign_in(USER2, PASSWORD2)

# --- nothing is there until an administrator makes a secret
code, state = admin.call("GET", "/api/ui/mail")
check("an administrator sees the intake's state", code == 200 and state["has_secret"] is False and "generic" in state["providers"], (code, state))
check("with the routes the plugin declares", {(r["plugin"], r["alias"]) for r in state["routes"]} >= {("tokplug", "support"), ("tokplug", "secure")}, state["routes"])
code, _ = post_mail("generic", mail(f"support@{DOMAIN}"), secret="nothing")
check("before there is a secret the address does not exist (404)", code == 404, code)
code, _ = bob.call("GET", "/api/ui/mail")
check("an ordinary member cannot manage the intake", code in (401, 403), code)
code, _ = bob.call("POST", "/api/ui/mail/secret", {})
check("nor make its secret", code in (401, 403), code)

code, made = admin.call("POST", "/api/ui/mail/secret", {})
secret = made.get("secret")
check("a secret is made, shown once", code == 201 and secret and made["rotated"] is False, (code, made))
setting(admin, "mail.inbound.domain", DOMAIN)
setting(admin, "mail.max_per_sender_hour", 1000)

# --- who may call
code, _ = post_mail("generic", mail(f"support@{DOMAIN}"))
check("without a secret: 401", code == 401, code)
code, _ = post_mail("generic", mail(f"support@{DOMAIN}"), secret="wrong")
check("with a wrong secret: 401", code == 401, code)
code, _ = post_mail("nosuch", mail(f"support@{DOMAIN}"), secret=secret)
check("an unknown provider: 404", code == 404, code)
code, _ = post_mail("generic", mail(f"support@{DOMAIN}"), secret=secret, org="no_such_org")
check("an unknown organization: 404", code == 404, code)
code, body = post_mail("generic", None, secret=secret, raw=b"not json")
check("a body that is not JSON is refused for good (422)", code == 422, (code, body))
code, body = post_mail("generic", {"subject": "no sender", "message_id": "x"}, secret=secret)
check("a message without a sender is refused for good (422)", code == 422, (code, body))
code, state = admin.call("GET", "/api/ui/mail")
check("refusals are counted, with the reason", state["rejected"] >= 2 and state["last_rejected_reason"], state)

# --- a message to a plugin's alias
code, body = post_mail("generic", mail(f"support@{DOMAIN}", "Printer", text="It is on fire"), secret=secret)
check("mail to an alias is accepted and routed", code == 200 and body["outcome"] == "routed", (code, body))
found = wait_memo(admin, "mail:support:Printer:ann@client.test:0:nil:It is on fire")
check("the plugin's function got a clean message (alias, subject, sender, text, no member)", len(found) == 1, memos(admin)[-3:])

first = mail(f"support@{DOMAIN}", "Same twice")
code, a = post_mail("generic", first, secret=secret)
code, b = post_mail("generic", first, secret=secret, basic=True)
check("a provider's retry creates nothing twice (also with Basic authentication)", a["outcome"] == "routed" and b["outcome"] == "duplicate", (a, b))
time.sleep(2)
check("the function ran once", len([m for m in memos(admin) if "Same twice" in m]) == 1)

code, body = post_mail("generic", mail([f"nobody@{DOMAIN}", f"Support+x@{DOMAIN}"], "Plus tag"), secret=secret)
check("a plus-tag and upper case still reach the alias, other recipients are skipped", body["outcome"] == "routed" and wait_memo(admin, "mail:support:Plus tag"), (code, body))

# attachments are stored through the media backend, with limits
att = [{"name": "note.txt", "type": "text/plain", "content_base64": base64.b64encode(b"hello attachment").decode()},
       {"name": "bad.exe", "type": "application/x-msdownload", "content_base64": base64.b64encode(b"MZ\x90\x00binary").decode()}]
code, body = post_mail("generic", mail(f"support@{DOMAIN}", "With files", attachments=att), secret=secret)
check("a message with attachments is routed", body["outcome"] == "routed", body)
check("the allowed file is stored, the other refused (one stored key reaches the plugin)", wait_memo(admin, "mail:support:With files:ann@client.test:1:"), memos(admin)[-2:])

# --- the unmatched queue
code, body = post_mail("generic", mail(f"stranger@{DOMAIN}", "Lost one"), secret=secret)
check("mail nobody has an alias for is kept, not dropped", body["outcome"] == "unmatched", body)
code, body = post_mail("generic", mail("support@other.test", "Wrong domain"), secret=secret)
check("mail to another domain is not ours: unmatched too", body["outcome"] == "unmatched", body)
code, listing = admin.call("GET", "/api/ui/mail/messages?status=unmatched")
subjects = {m["subject"]: m for m in listing["messages"]}
check("the administrator sees them, with the reason", {"Lost one", "Wrong domain"} <= set(subjects) and subjects["Lost one"]["reason"], listing)
lost = subjects["Lost one"]["key"]
code, body = admin.call("POST", f"/api/ui/mail/messages/{lost}/assign", {"plugin": "tokplug", "alias": "support"})
check("an unmatched message can be given to a plugin's alias", code == 200, (code, body))
check("and the function runs with it", wait_memo(admin, "mail:support:Lost one"), memos(admin)[-2:])
code, body = admin.call("POST", f"/api/ui/mail/messages/{lost}/assign", {"plugin": "tokplug", "alias": "support"})
check("only once", code == 422, (code, body))
code, body = admin.call("POST", f"/api/ui/mail/messages/{subjects['Wrong domain']['key']}/assign", {"plugin": "tokplug", "alias": "nosuch"})
check("an alias that does not exist is refused", code == 422, (code, body))
code, body = admin.call("POST", f"/api/ui/mail/messages/{subjects['Wrong domain']['key']}/discard", {})
check("a message can be set aside", code == 200, (code, body))
code, listing = admin.call("GET", "/api/ui/mail/messages?status=ignored")
check("and shows as ignored with the reason", any(m["subject"] == "Wrong domain" and "administrator" in (m["reason"] or "") for m in listing["messages"]), listing)
code, body = admin.call("POST", "/api/ui/mail/messages/bad%20key%21/discard", {})
check("a key with odd characters is refused", code in (400, 404), (code, body))

# --- mail that no person wrote
for name, extra in [("Auto-Submitted", {"headers": {"Auto-Submitted": "auto-replied"}}), ("bulk", {"headers": {"Precedence": "bulk"}}),
                    ("list", {"headers": {"List-Id": "<news.example.test>"}})]:
    code, body = post_mail("generic", mail(f"support@{DOMAIN}", f"auto {name}", **extra), secret=secret)
    check(f"{name} mail is set aside, not routed", body["outcome"] == "ignored", body)
code, body = post_mail("generic", mail(f"support@{DOMAIN}", "from daemon", sender="mailer-daemon@client.test"), secret=secret)
check("a bounce from a system address is set aside", body["outcome"] == "ignored", body)
time.sleep(1)
check("none of them reached the plugin", not [m for m in memos(admin) if "auto " in m or "from daemon" in m])

# --- what the sender's domain proved
code, body = post_mail("generic", mail(f"secure@{DOMAIN}", "Unproven", auth={"spf": "pass", "dkim": "none"}), secret=secret)
check("a route that requires DKIM keeps mail without it for an administrator", body["outcome"] == "unmatched", body)
code, body = post_mail("generic", mail(f"secure@{DOMAIN}", "Proven", auth={"spf": "pass", "dkim": "pass"}), secret=secret)
check("and takes mail that has it", body["outcome"] == "routed" and wait_memo(admin, "mail:secure:Proven"), body)

# --- the provider's own format
postmark = {"FromFull": {"Email": "Pat@client.test", "Name": "Pat"}, "ToFull": [{"Email": f"support@{DOMAIN}", "Name": ""}], "Subject": "From Postmark",
            "MessageID": "pm-1", "TextBody": "hi from postmark", "HtmlBody": "<p>hi <script>x()</script></p>",
            "Headers": [{"Name": "Message-ID", "Value": f"<pm.{RUN}@client.test>"}, {"Name": "Received-SPF", "Value": "Pass"}],
            "Attachments": [{"Name": "a.txt", "ContentType": "text/plain", "Content": base64.b64encode(b"hello").decode()}]}
code, body = post_mail("postmark", postmark, secret=secret, basic=True)
check("Postmark's format is read (authenticated by Basic, as Postmark can)", code == 200 and body["outcome"] == "routed", (code, body))
check("with its attachment", wait_memo(admin, "mail:support:From Postmark:pat@client.test:1:"), memos(admin)[-2:])

# --- replies find their record
code, contact = admin.call("POST", P + "new_contact", {"name": "Dana", "email": f"dana.{RUN}@x.test", "owner": (lambda me: me.get("id") or me.get("user", {}).get("id"))(admin.call("GET", "/api/auth/me")[1])})
check("a record to talk about", code == 200 and contact.get("id"), (code, contact))
record = contact["id"]
key = record.split(":")[-1]
code, tok = admin.call("POST", P + "mail_token", {"record": record, "alias": "support"})
check("a plugin gets the record's mail token and Reply-To address", code == 200 and tok["reply_to"] == f"support+tk_{tok['token']}@{DOMAIN}", (code, tok))
code, again = admin.call("POST", P + "mail_token", {"record": record})
check("the token stays the same", again["token"] == tok["token"] and "reply_to" not in again, again)
code, body = admin.call("POST", P + "mail_token", {"record": "contact:doesnotexist"})
check("a record that does not exist has none", code != 200, (code, body))
code, body = bob.call("POST", P + "mail_token", {"record": record})
check("a person who may not read the record cannot get its token", code != 200, (code, body))

before = len(memos(admin))
code, body = post_mail("generic", mail(tok["reply_to"], "Re: Quote", text="Yes, please go ahead"), secret=secret)
check("a reply to that address is appended to the record, not sent to the function", body["outcome"] == "replied", body)
code, thread = admin.call("GET", f"/api/chatter/tokplug/contact/{key}")
lines = [m for m in thread.get("messages", []) if "go ahead" in (m.get("body") or "")]
check("the record's chatter has it, from the sender", code == 200 and len(lines) == 1 and lines[0]["author_label"] in ("Ann", "ann@client.test"), (code, str(thread)[:400]))
time.sleep(1)
check("the plugin's function was not called", len(memos(admin)) == before)

code, body = post_mail("generic", mail(f"someone@{DOMAIN}", "Re: Quote", text="Replying by header only", in_reply_to=f"<tk_{tok['token']}.3@{DOMAIN}>"), secret=secret)
check("a reply is found by its In-Reply-To too", body["outcome"] == "replied", body)
code, body = post_mail("generic", mail(tok["reply_to"], "Re: Quote", text="forged", auth={"spf": "fail", "dkim": "fail"}), secret=secret)
check("a reply failing both SPF and DKIM goes to the administrator instead", body["outcome"] == "unmatched", body)
code, body = post_mail("generic", mail(f"support+tk_aaaaaaaaaa@{DOMAIN}", "Re: guess", text="a guessed token"), secret=secret)
check("an unknown token falls back to the alias route", body["outcome"] == "routed", body)
code, thread = admin.call("GET", f"/api/chatter/tokplug/contact/{key}")
bodies = [m.get("body") for m in thread.get("messages", [])]
check("only the real replies are in the chatter", any("go ahead" in (b or "") for b in bodies) and any("header only" in (b or "") for b in bodies) and not any("forged" in (b or "") or "guessed" in (b or "") for b in bodies), bodies)

# --- floods
setting(admin, "mail.max_per_sender_hour", 3)
outcomes = [post_mail("generic", mail(f"support@{DOMAIN}", f"flood {i}", sender=f"flooder.{RUN}@x.test"), secret=secret)[1]["outcome"] for i in range(5)]
check("a sender over the hourly limit has the rest set aside", outcomes == ["routed", "routed", "routed", "ignored", "ignored"], outcomes)
setting(admin, "mail.max_per_sender_hour", 1000)

# --- counters, rotation, switching off
code, state = admin.call("GET", "/api/ui/mail")
counts = {row["status"]: row["n"] for row in state["messages"]}
check("the state counts what came in and what became of it", counts.get("routed", 0) >= 8 and counts.get("replied") == 2 and counts.get("unmatched", 0) >= 2 and counts.get("ignored", 0) >= 6 and state["received"] >= 20, state)
code, rotated = admin.call("POST", "/api/ui/mail/secret", {})
new_secret = rotated["secret"]
check("rotating gives a new secret and keeps the old one for a while", code == 200 and rotated["rotated"] is True and new_secret != secret, (code, rotated))
check("both work during the overlap", post_mail("generic", mail(f"support@{DOMAIN}", "overlap A"), secret=secret)[0] == 200 and post_mail("generic", mail(f"support@{DOMAIN}", "overlap B"), secret=new_secret)[0] == 200)
code, body = admin.call("PATCH", "/api/ui/mail", {"enabled": False})
check("intake can be switched off", code == 200, (code, body))
code, _ = post_mail("generic", mail(f"support@{DOMAIN}", "while off"), secret=new_secret)
check("and then the address does not exist (404)", code == 404, code)
admin.call("PATCH", "/api/ui/mail", {"enabled": True})
code, _ = post_mail("generic", mail(f"support@{DOMAIN}", "back on"), secret=new_secret)
check("and works again when switched on", code == 200, code)

setting(admin, "mail.inbound.domain", "")
finish()
