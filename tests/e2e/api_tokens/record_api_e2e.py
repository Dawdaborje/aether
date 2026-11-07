"""The record API, end to end over HTTP: see README.md in this folder for the one-time setup.

Run: python3 record_api_e2e.py. Needs the throwaway instance with the plugin `tokplug` (model `lead`,
offered in `[[api]]`; model `memo`, not offered), the organization owner (an administrator) and a second,
ordinary member `bob` (AETHER_USER2 / AETHER_PASSWORD2). Makes its own records and tokens each run.
Exits 1 when a check fails."""
import json
import os
from urllib.parse import quote

from e2e_lib import *

USER2 = os.environ.get("AETHER_USER2", "bob")
PASSWORD2 = os.environ.get("AETHER_PASSWORD2", "Test-pass-123")


def sign_in(user, password):
    b = Browser()
    b.call("GET", "/api/auth/methods")
    code, body = b.call("POST", "/api/auth/login", {"username": user, "password": password})
    check(f"{user} signs in", code == 200, (code, body))
    code, me = b.call("GET", "/api/auth/me")
    return b, me


def make_token(b, name, scopes):
    code, body = b.call("POST", "/api/ui/tokens", {"name": f"{name} {RUN}", "scopes": scopes})
    check(f"a token '{name}' is made", code == 201, (code, body))
    return body.get("token")


owner_b, owner_me = sign_in(USER, PASSWORD)
bob_b, bob_me = sign_in(USER2, PASSWORD2)
owner_id = owner_me.get("id") or owner_me.get("user", {}).get("id")
bob_id = bob_me.get("id") or bob_me.get("user", {}).get("id")
check("both people have ids", bool(owner_id) and bool(bob_id), (owner_me, bob_me))

full = make_token(owner_b, "owner full", {"models": ["tokplug.lead:read,create,update,delete"]})
readonly = make_token(owner_b, "owner read-only", {"models": ["tokplug.lead:read"]})
plugin_only = make_token(owner_b, "owner plugin", {"plugins": ["tokplug"]})
bob_tok = make_token(bob_b, "bob", {"models": ["tokplug.lead:read,create,update,delete", "tokplug.memo:read"]})

L = "/api/v1/tokplug/lead"

# --- create, read, list
code, a = machine("POST", L, full, {"title": f"A {RUN}", "owner": owner_id, "amount": "10.50", "secret": "s3"})
check("a record is created (201)", code == 201 and a["data"]["title"] == f"A {RUN}" and a["data"]["amount"] == "10.50", (code, a))
aid = a["data"]["id"]
code, b_ = machine("POST", L, full, {"title": f"B {RUN}", "owner": owner_id, "status": "won"})
check("a second record is created", code == 201, (code, b_))
code, body = machine("GET", f"{L}/{aid}", full)
check("a record is read by id", code == 200 and body["data"]["id"] == aid, (code, body))
code, body = machine("GET", f"{L}/{aid.split(':')[-1]}", full)
check("the id may be written without the table part", code == 200, (code, body))
code, body = machine("GET", f"{L}/nosuchrecord", full)
check("an unknown record is a 404", code == 404 and body["error"]["code"] == "not_found", (code, body))

code, body = machine("GET", f"{L}?filter={quote(json.dumps({'status': 'won'}))}", full)
titles = [r["title"] for r in body.get("data", [])]
check("a filter narrows the list", code == 200 and f"B {RUN}" in titles and f"A {RUN}" not in titles, (code, body))
code, body = machine("GET", f"{L}?fields=title", full)
check("fields picks the columns", code == 200 and all(set(r) <= {"id", "title"} for r in body["data"]), (code, body))
code, body = machine("GET", f"{L}?limit=0", full)
check("a bad limit is refused", code == 422 and body["error"]["code"] == "invalid", (code, body))
code, body = machine("GET", f"{L}?filter=notjson", full)
check("a bad filter is refused", code == 422, (code, body))
code, body = machine("GET", f"{L}?filter={quote(json.dumps({'nofield': 1}))}", full)
check("a filter on an unknown field is refused", code == 422, (code, body))

# paging: walk the whole list one at a time
seen, after = [], None
for _ in range(50):
    code, page = machine("GET", f"{L}?limit=1" + (f"&after={after}" if after else ""), full)
    check("a page is returned", code == 200, (code, page)) if code != 200 else None
    seen += [r["id"] for r in page["data"]]
    after = page["next"]
    if not after:
        break
check("paging by `after` visits every record exactly once, in order", len(seen) == len(set(seen)) and seen == sorted(seen) and aid in seen, seen)

# --- change and delete
code, body = machine("PATCH", f"{L}/{aid}", full, {"status": "won", "amount": "11.00"})
check("a record is changed", code == 200 and body["data"]["status"] == "won" and body["data"]["amount"] == "11.00", (code, body))
code, body = machine("PATCH", f"{L}/{aid}", full, {"status": "nonsense"})
check("a bad value is refused (422)", code == 422, (code, body))
code, body = machine("PATCH", f"{L}/{aid}", full, {})
check("an empty body is refused", code == 422, (code, body))
code, body = machine("POST", L, full, {"owner": owner_id})
check("a missing required field is refused", code == 422, (code, body))
code, body = machine("POST", L, full, {"title": "x", "owner": owner_id, "nosuchfield": 1})
check("an unknown field is refused", code == 422, (code, body))
code, body = machine("DELETE", f"{L}/{b_['data']['id']}", full)
check("a record is deleted", code == 200 and body["deleted"], (code, body))
code, body = machine("GET", f"{L}/{b_['data']['id']}", full)
check("and is gone", code == 404, (code, body))

# --- what a token's scope allows
code, body = machine("GET", L, readonly)
check("a read-only token lists", code == 200, (code, body))
code, body = machine("POST", L, readonly, {"title": "no", "owner": owner_id})
check("a read-only token cannot create", code == 403 and body["error"]["code"] == "token_scope", (code, body))
code, body = machine("DELETE", f"{L}/{aid}", readonly)
check("nor delete", code == 403, (code, body))
code, body = machine("GET", L, plugin_only)
check("a token for calling functions is not a token for records", code == 403 and body["error"]["code"] == "token_scope", (code, body))
code, body = machine("GET", L, None)
check("no credential is refused", code in (401, 403), (code, body))

# --- what the plugin offers
code, body = machine("GET", "/api/v1/tokplug/memo", bob_tok)
check("a model the plugin does not offer is a 404", code == 404 and body["error"]["code"] == "not_found", (code, body))
code, body = machine("GET", "/api/v1/nosuchplugin/lead", full)
check("an unknown plugin is refused", code in (403, 404), (code, body))

# --- the person's own rules apply: bob sees only his own records, and not the secret field
code, mine = machine("POST", L, bob_tok, {"title": f"Bob's {RUN}", "owner": bob_id})
check("bob creates a record of his own", code == 201, (code, mine))
code, forged = machine("POST", L, bob_tok, {"title": "forged", "owner": owner_id})
check("bob cannot create one for someone else (the rule refuses)", code == 403, (code, forged))
code, body = machine("GET", L, bob_tok)
check("bob lists only his own records", code == 200 and f"Bob's {RUN}" in [r["title"] for r in body["data"]] and all(r["owner"] == bob_id for r in body["data"]), (code, body))
check("and does not see the field his role hides", all("secret" not in r for r in body["data"]), body)
code, forged = machine("POST", L, bob_tok, {"title": "sneaky", "owner": bob_id, "secret": "bobs"})
check("bob may not set that field even on his own record", code == 403 and "secret" in forged["error"]["message"], (code, forged))
code, body = machine("GET", f"{L}/{aid}", bob_tok)
check("bob cannot read the owner's record (404, as if absent)", code == 404, (code, body))
code, body = machine("PATCH", f"{L}/{aid}", bob_tok, {"title": "hijack"})
check("nor change it", code == 404, (code, body))
code, body = machine("DELETE", f"{L}/{aid}", bob_tok)
check("nor delete it", code == 404, (code, body))
code, body = machine("GET", f"{L}/{aid}", full)
check("the owner's record is untouched", code == 200 and body["data"]["title"] == f"A {RUN}", (code, body))
code, body = machine("PATCH", f"{L}/{mine['data']['id']}", bob_tok, {"secret": "x"})
check("bob may not set the field his role locks", code == 403, (code, body))

# --- the generated description
code, spec = machine("GET", "/api/v1/tokplug/openapi.json", full)
check("the owner gets an OpenAPI document", code == 200 and spec.get("openapi", "").startswith("3."), (code, spec))
paths = spec.get("paths", {})
check("it describes every operation the token has", set(paths.get(L, {})) >= {"get", "post"} and set(paths.get(L + "/{id}", {})) >= {"get", "patch", "delete"}, list(paths))
check("it does not describe a model that is not offered", not any("memo" in p for p in paths), list(paths))
check("it shows the admin the field the role hides", "secret" in spec["components"]["schemas"]["Lead"]["properties"], spec["components"]["schemas"])
check("and the choices of a select field", spec["components"]["schemas"]["Lead"]["properties"]["status"]["enum"] == ["new", "won", "lost"], spec["components"]["schemas"]["Lead"])
code, spec = machine("GET", "/api/v1/tokplug/openapi.json", readonly)
methods = lambda path: set(spec["paths"].get(path, {})) - {"parameters"}
check("a read-only token's document has no write operations", methods(L) == {"get"} and methods(L + "/{id}") == {"get"}, spec["paths"])
code, spec = machine("GET", "/api/v1/tokplug/openapi.json", bob_tok)
check("bob's document leaves out the field he cannot see", "secret" not in spec["components"]["schemas"]["Lead"]["properties"], spec["components"]["schemas"]["Lead"])
check("and does not offer it as input", "secret" not in spec["components"]["schemas"]["LeadInput"]["properties"], spec["components"]["schemas"]["LeadInput"])

# --- a signed-in browser may use it too, with its CSRF token
code, body = owner_b.call("GET", f"{L}/{aid}")
check("a browser session reads through the API", code == 200, (code, body))
code, body = owner_b.call("PATCH", f"{L}/{aid}", {"title": f"A2 {RUN}"})
check("and changes with its CSRF token", code == 200, (code, body))
code, body = owner_b.call("PATCH", f"{L}/{aid}", {"title": "no csrf"}, csrf=False)
check("but not without it", code == 403 and body == {"error": "csrf"}, (code, body))

finish()
