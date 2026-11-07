"""API tokens, end to end over HTTP: see README.md in this folder for the one-time setup.

Run: python3 api_tokens_e2e.py. It makes its own tokens each run (named with a run id), so it can be
repeated. Exits 1 when a check fails."""
import json
from e2e_lib import *

# --- a person signs in and makes tokens
b = Browser()
b.call("GET", "/api/auth/methods")
code, body = b.call("POST", "/api/auth/login", {"username": USER, "password": PASSWORD})
check("the owner signs in", code == 200, (code, body))

code, body = b.call("POST", "/api/ui/tokens", {"name": "no scopes", "scopes": {}})
check("a token with no scopes is refused", code == 422, (code, body))
code, body = b.call("POST", "/api/ui/tokens", {"name": "x", "scopes": {"models": ["lead"]}})
check("a malformed scope is refused", code == 422, (code, body))
code, body = b.call("POST", "/api/ui/tokens", {"name": "too long", "scopes": {"plugins": ["tokplug"]}, "expires_days": 99999})
check("a token cannot outlast the organization's limit", code == 422, (code, body))

code, plug = b.call("POST", "/api/ui/tokens", {"name": "whole plugin " + RUN, "scopes": {"plugins": ["tokplug"]}})
check("a token for a plugin is made", code == 201 and plug.get("token", "").startswith("aet_"), (code, plug))
code, one = b.call("POST", "/api/ui/tokens", {"name": "one function " + RUN, "scopes": {"functions": ["tokplug.ping"]}, "expires_days": 7})
check("a token for one function is made", code == 201, (code, one))
code, listing = b.call("GET", "/api/ui/tokens")
names = sorted(t["name"] for t in listing.get("tokens", []) if t["name"].endswith(RUN))
check("the list shows both, without secrets", code == 200 and names == ["one function " + RUN, "whole plugin " + RUN] and plug["token"] not in json.dumps(listing), (code, listing))

# --- a script uses them
code, body = machine("POST", "/api/plugins/tokplug/ping", plug["token"], {"n": 7})
check("a script calls a function with only the bearer (no cookie, no CSRF)", code == 200 and body.get("pong") is True and body.get("n") == 7, (code, body))
code, body = machine("POST", "/api/plugins/tokplug/other", plug["token"], {})
check("the plugin-wide token reaches the plugin's other function", code == 200 and body.get("other") is True, (code, body))
code, body = machine("POST", "/api/plugins/tokplug/ping", one["token"], {"n": 1})
check("the one-function token calls its function", code == 200, (code, body))
code, body = machine("POST", "/api/plugins/tokplug/other", one["token"], {})
check("the one-function token may not call another", code == 403 and "may not call" in json.dumps(body), (code, body))
code, body = machine("POST", "/api/plugins/nosuch/ping", one["token"], {})
check("nor another plugin's", code == 403, (code, body))

# --- what a token cannot do
code, body = machine("POST", "/api/plugins/tokplug/ping", "aet_" + "0" * 64, {"n": 1})
check("an unknown token is refused", code == 401, (code, body))
code, body = machine("POST", "/api/plugins/tokplug/ping", None, {"n": 1})
check("no token and no session is refused (as a browser request, by the CSRF check)", code in (401, 403), (code, body))
code, body = machine("POST", "/api/plugins/tokplug/ping", "not-our-token", {"n": 1})
check("a bearer token of another system is refused (it is not ours, so the browser checks apply)", code in (401, 403), (code, body))
code, body = machine("GET", "/api/ui/tokens", plug["token"])
check("a token is not accepted on the rest of the API", code in (401, 403), (code, body))
code, body = machine("POST", "/api/ui/tokens", plug["token"], {"name": "child", "scopes": {"plugins": ["tokplug"]}})
check("a token cannot make tokens (it has no browser to pass the CSRF check)", code in (401, 403), (code, body))
code, body = machine("POST", "/api/auth/logout", plug["token"], {})
check("nor reach the auth endpoints", code in (401, 403), (code, body))

# --- the browser keeps its checks, even if it also sends a token
b2 = Browser()
b2.call("GET", "/api/auth/methods")
b2.call("POST", "/api/auth/login", {"username": USER, "password": PASSWORD})
code, body = b2.call("POST", "/api/plugins/tokplug/ping", {"n": 1}, csrf=False, headers={"Authorization": "Bearer " + plug["token"]})
check("a session cookie plus a token still needs the CSRF token", code == 403 and body == {"error": "csrf"}, (code, body))
code, body = b2.call("POST", "/api/plugins/tokplug/ping", {"n": 1})
check("the browser's own call works", code == 200, (code, body))

# --- the audit trail says which token
code, body = machine("POST", "/api/plugins/tokplug/ping", plug["token"], {"n": 2}, {"User-Agent": "my-script/1.0"})
check("(another call, with a user agent)", code == 200)

# --- revoking
code, listing = b.call("GET", "/api/ui/tokens")
tid = next(t["id"] for t in listing["tokens"] if t["name"] == "whole plugin " + RUN)
code, body = b.call("DELETE", f"/api/ui/tokens/{tid}")
check("the person revokes a token", code == 200, (code, body))
code, body = machine("POST", "/api/plugins/tokplug/ping", plug["token"], {"n": 1})
check("the revoked token stops working at once", code == 401, (code, body))
code, body = b.call("DELETE", f"/api/ui/tokens/{tid}")
check("revoking twice is a 404", code == 404, (code, body))
code, body = machine("POST", "/api/plugins/tokplug/ping", one["token"], {"n": 1})
check("the other token is unaffected", code == 200, (code, body))

finish()
