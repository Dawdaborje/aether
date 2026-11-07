"""The realtime WebSocket, end to end: see README.md for the one-time setup.

Needs `websocket-client` (pip install websocket-client). Makes its own channels each run, so it can be repeated.
Exits 1 when a check fails."""
import json, time
import websocket
from e2e_lib import *

WS = BASE.replace("http://", "ws://") + "/api/ui/realtime"


def login(user, password=PASSWORD):
    b = Browser()
    b.call("GET", "/api/auth/methods")
    code, body = b.call("POST", "/api/auth/login", {"username": user, "password": password})
    assert code == 200, (user, code, body)
    return b


def cookie_header(b):
    return "; ".join(f"{c.name}={c.value}" for c in b.jar)


def connect(b, origin=BASE, cookie=True, timeout=3):
    headers = []
    if cookie:
        headers.append("Cookie: " + cookie_header(b))
    return websocket.create_connection(WS, header=headers, origin=origin, timeout=timeout)


def refused(b, **kw):
    try:
        connect(b, **kw).close()
        return None
    except websocket.WebSocketBadStatusException as e:
        return e.status_code
    except Exception as e:  # noqa: BLE001
        return repr(e)


def send(ws, **frame):
    ws.send(json.dumps(frame))


def frames(ws, wait=0.4):
    """Everything that arrives in the next `wait` seconds."""
    out = []
    end = time.time() + wait
    while time.time() < end:
        ws.settimeout(max(0.05, end - time.time()))
        try:
            data = ws.recv()
            if not data:  # the server closed the connection
                out.append({"t": "closed"})
                break
            out.append(json.loads(data))
        except websocket.WebSocketTimeoutException:
            break
        except websocket.WebSocketConnectionClosedException:
            out.append({"t": "closed"})
            break
    return out


def of(fs, t):
    return [f for f in fs if f.get("t") == t]


def call(b, fn, body):
    code, out = b.call("POST", f"/api/plugins/chatplug/{fn}", body)
    assert code == 200, (fn, code, out)
    return out


owner = login(USER)
bob = login("bob")

# --- who may connect
check("a connection without a session is refused", refused(owner, cookie=False) == 401, refused(owner, cookie=False))
check("a connection from another site is refused", refused(owner, origin="https://evil.example") == 403)
try:
    bare = websocket.create_connection(WS, header=["Cookie: " + cookie_header(owner)], suppress_origin=True, timeout=3)
    bare.close()
    check("a connection with no Origin is refused", False, "connected")
except websocket.WebSocketBadStatusException as e:
    check("a connection with no Origin is refused", e.status_code == 403, e.status_code)

# --- topics and who may join
public = call(owner, "make_channel", {"name": "general " + RUN})["id"]
private = call(owner, "make_channel", {"name": "secret " + RUN, "private": True})["id"]
topic = f"chatplug:channel.{public}"
secret = f"chatplug:channel.{private}"

a = connect(owner)
b = connect(bob)
send(a, op="ping")
check("the server answers a ping", of(frames(a), "pong") != [])

send(a, op="sub", topic=topic)
fa = frames(a)
sub = of(fa, "sub")
check("the owner joins a public channel", sub and sub[0]["ok"] is True and sub[0]["seq"] == 0, fa)
owner_id = sub[0]["presence"][0] if sub and sub[0].get("presence") else None
check("presence lists the owner", bool(owner_id), fa)

send(b, op="sub", topic=topic)
fb = frames(b)
check("bob joins too and sees the owner is here", of(fb, "sub") and owner_id in of(fb, "sub")[0]["presence"], fb)
joins = of(frames(a), "presence")
check("the owner is told bob arrived", joins and joins[0].get("join") and joins[0]["join"] != owner_id, joins)
bob_id = joins[0]["join"] if joins else None

for bad, why in [("chatplug:nosuch.1", "unknown topic"), ("chatplug:locked", "locked"), ("other:channel.1", "unknown topic"), ("nocolon", "unknown topic")]:
    send(b, op="sub", topic=bad)
    r = of(frames(b), "sub")
    check(f"joining {bad} is refused ({why})", r and r[0]["ok"] is False and r[0]["reason"] == why, r)

send(b, op="sub", topic=secret)
r = of(frames(b), "sub")
check("a private channel refuses a non-member", r and r[0]["ok"] is False and r[0]["reason"] == "private channel", r)
call(bob, "join_channel", {"channel": private})
send(b, op="sub", topic=secret)
r = of(frames(b), "sub")
check("once added, bob may join it", r and r[0]["ok"] is True, r)
send(b, op="unsub", topic=secret)

# --- messages
m1 = call(owner, "post_message", {"channel": public, "body": "hello"})
fa, fb = frames(a), frames(b)
ma, mb = of(fa, "msg"), of(fb, "msg")
check("both members get the message", ma and mb and mb[0]["payload"]["row"]["body"] == "hello", (fa, fb))
check("the message carries its sequence number", mb and mb[0]["seq"] == 1 and m1["seq"] == 1, (mb, m1))
check("the stored row came with it", mb and mb[0]["payload"]["row"]["n"] == 1, mb)
send(a, op="unsub", topic=topic)
call(owner, "post_message", {"channel": public, "body": "second"})
check("after leaving, the owner gets nothing more", of(frames(a), "msg") == [])
send(a, op="sub", topic=topic, since=1)
fa = frames(a)
check("rejoining from the last sequence replays what was missed", [f["seq"] for f in of(fa, "msg")] == [2], fa)
check("...in order, after the join answer", fa and fa[0]["t"] == "sub", fa)
frames(b)

# --- typing and other client events
send(b, op="event", topic=topic, kind="typing")
fa, fb = frames(a), frames(b)
ev = of(fa, "event")
check("a typing signal reaches the others, with who sent it", ev and ev[0]["kind"] == "typing" and ev[0]["user"] == bob_id, fa)
check("the sender does not get its own typing back", of(fb, "event") == [], fb)
send(b, op="event", topic=topic, kind="shout")
check("a kind the plugin did not declare is dropped", of(frames(a), "event") == [])
for _ in range(40):
    send(b, op="event", topic=topic, kind="typing")
got = len(of(frames(a, 1.0), "event"))
check("a burst of signals is rate limited", 0 < got <= 12, got)

# --- presence
who = call(owner, "who_is_here", {"channel": public})
check("a plugin can ask who is connected", sorted(who["users"]) == sorted([owner_id, bob_id]), who)
b.close()
left = of(frames(a, 0.8), "presence")
check("the others are told when the last connection of a person closes", left and left[0].get("leave") == bob_id, left)
who = call(owner, "who_is_here", {"channel": public})
check("...and presence forgets them", who["users"] == [owner_id], who)

# --- resuming beyond what the server keeps
b = connect(bob)
send(b, op="sub", topic=topic)
frames(b)
for i in range(8):
    call(owner, "post_message", {"channel": public, "body": f"burst {i}"})
frames(a), frames(b)
b.close()
call(owner, "post_message", {"channel": public, "body": "while away"})
b = connect(bob)
send(b, op="sub", topic=topic, since=2)
fb = frames(b)
check("too far behind: the server says to read from the database", of(fb, "resync") != [], fb)
send(b, op="sub", topic=topic, since=99999)
fb = frames(b)
check("a sequence from the future also resyncs", of(fb, "resync") != [], fb)

# --- being removed
send(b, op="sub", topic=secret)
frames(b)
call(bob, "leave_channel", {"channel": private})
fb = frames(b)
check("when the plugin drops a member they are told", of(fb, "dropped") != [], fb)
call(owner, "post_message", {"channel": private, "body": "after"})
check("...and hear nothing more", of(frames(b), "msg") == [])

# --- source function the widget uses
code, page = owner.call("POST", "/api/plugins/chatplug/channel_messages", {"channel": public, "limit": 5})
rows = page["rows"] if code == 200 else []
check("the widget's source answers oldest first with more=true", code == 200 and [r["n"] for r in rows] == sorted(r["n"] for r in rows) and page["more"] is True, page)
code, older = owner.call("POST", "/api/plugins/chatplug/channel_messages", {"channel": public, "limit": 50, "before": rows[0]["id"]})
check("`before` returns the rows above the oldest held", code == 200 and all(r["n"] < rows[0]["n"] for r in older["rows"]), older)

# --- limits
extra = []
try:
    for _ in range(9):
        extra.append(connect(owner))
    check("a ninth connection of one person is refused", False, "all connected")
except websocket.WebSocketBadStatusException as e:
    check("a ninth connection of one person is refused", e.status_code == 429, e.status_code)
for w in extra:
    w.close()
send(a, op="nonsense")
check("an unknown op is answered, not fatal", of(frames(a), "error") != [])
a.send("not json")
check("a frame that is not JSON is answered, not fatal", of(frames(a), "error") != [])
a.send(json.dumps({"op": "event", "topic": topic, "kind": "typing", "payload": "x" * 40000}))
closed = frames(a, 1.0)
check("a frame over 16 KB closes the connection", of(closed, "closed") != [], closed)

for w in (a, b):
    try:
        w.close()
    except Exception:  # noqa: BLE001
        pass
finish()
