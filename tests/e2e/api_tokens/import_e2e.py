"""Imports and exports, end to end over HTTP: see README.md in this folder for the one-time setup.

Run: python3 import_e2e.py. It uploads files the way a browser does, starts imports through the plugin
`tokplug` (model `contact`, whose rules let an ordinary member see and write only their own), and checks
the dry run, the real run, upsert by key, the report, the file of failed rows, the callback, undo, XLSX,
a large file, and an export that imports back unchanged. Needs an organization administrator
(AETHER_USER) and an ordinary member (AETHER_USER2). Exits 1 when a check fails."""
import io
import json
import os
import time
import urllib.error
import urllib.request
import zipfile

from e2e_lib import *

USER2 = os.environ.get("AETHER_USER2", "bob")
PASSWORD2 = os.environ.get("AETHER_PASSWORD2", "Test-pass-123")
P = "/api/plugins/tokplug/"


def sign_in(user, password):
    b = Browser()
    b.call("GET", "/api/auth/methods")
    code, body = b.call("POST", "/api/auth/login", {"username": user, "password": password})
    check(f"{user} signs in", code == 200, (code, body))
    code, me = b.call("GET", "/api/auth/me")
    return b, me.get("id") or me.get("user", {}).get("id")


def fn(browser, name, payload=None):
    return browser.call("POST", P + name, payload or {})


def upload(browser, name, data, content_type="text/csv"):
    request = urllib.request.Request(
        BASE + "/api/media?name=" + name, data=data, method="POST",
        headers={"Content-Type": content_type, "Origin": BASE, "X-CSRF-Token": browser.csrf() or ""},
    )
    try:
        with browser.opener.open(request) as response:
            return response.status, json.loads(response.read())
    except urllib.error.HTTPError as error:
        return error.code, error.read().decode(errors="replace")


def wait_for(browser, job, states=("done", "failed", "undone"), seconds=240):
    end = time.time() + seconds
    while time.time() < end:
        code, status = fn(browser, "import_status", {"job": job})
        if code == 200 and status.get("status") in states:
            return status
        time.sleep(0.4)
    return fn(browser, "import_status", {"job": job})[1]


def run_import(browser, name, text, mode="dry_run", mapping=None, **extra):
    code, up = upload(browser, name, text.encode() if isinstance(text, str) else text)
    check(f"{name} is uploaded", code == 200 and up.get("key"), (code, up))
    code, started = fn(browser, "import_start", {"media_key": up["key"], "mode": mode, "mapping": mapping, **extra})
    check(f"{name}: {mode} is queued", code == 200 and started.get("job"), (code, started))
    return started.get("job"), up["key"], wait_for(browser, started.get("job"))


def count(browser):
    return fn(browser, "count_contacts")[1].get("n")


def rows_of(browser, job, only_errors=False):
    out, offset = [], 0
    while True:
        code, body = fn(browser, "import_result", {"job": job, "offset": offset, "limit": 500, "only_errors": only_errors})
        out += body["rows"]
        if body.get("next_offset") is None:
            return out
        offset = body["next_offset"]


def xlsx(rows):
    """A spreadsheet file made by hand: shared strings for text, plain numbers."""
    strings, cells = [], []
    for r, row in enumerate(rows, 1):
        out = []
        for c, value in enumerate(row):
            ref = "ABCDEFGH"[c] + str(r)
            if isinstance(value, (int, float)):
                out.append(f'<c r="{ref}"><v>{value}</v></c>')
            else:
                strings.append(value)
                out.append(f'<c r="{ref}" t="s"><v>{len(strings) - 1}</v></c>')
        cells.append(f'<row r="{r}">{"".join(out)}</row>')
    buffer = io.BytesIO()
    with zipfile.ZipFile(buffer, "w", zipfile.ZIP_DEFLATED) as z:
        z.writestr("[Content_Types].xml", '<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="xml" ContentType="application/xml"/><Override PartName="/xl/workbook.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml"/></Types>')
        z.writestr("xl/workbook.xml", '<workbook xmlns:r="r"><sheets><sheet name="Contacts" sheetId="1" r:id="rId1"/></sheets></workbook>')
        z.writestr("xl/_rels/workbook.xml.rels", '<Relationships><Relationship Id="rId1" Target="worksheets/sheet1.xml"/></Relationships>')
        z.writestr("xl/sharedStrings.xml", "<sst>" + "".join(f"<si><t>{s}</t></si>" for s in strings) + "</sst>")
        z.writestr("xl/worksheets/sheet1.xml", "<worksheet><sheetData>" + "".join(cells) + "</sheetData></worksheet>")
    return buffer.getvalue()


admin, admin_id = sign_in(USER, PASSWORD)
bob, bob_id = sign_in(USER2, PASSWORD2)
tag = RUN  # makes the emails of this run unique, so the script can be repeated
mail = lambda n: f"{n}.{tag}@import.test"
OWN = admin_id

# --- templates and guessing
code, tpl = fn(admin, "import_templates")
names = [f["name"] for f in tpl.get("fields", [])]
check("a template lists the model's fields as a CSV header", code == 200 and tpl["csv"].strip() == ",".join(names) and "name" in names and "id" not in names, (code, tpl))
check("and says what is required and which choices exist", next(f for f in tpl["fields"] if f["name"] == "name")["required"] is True and next(f for f in tpl["fields"] if f["name"] == "status")["choices"] == ["new", "won", "lost"], tpl)
code, g = fn(admin, "import_guess", {"headers": ["Full name", "E-mail", "Amount", "Joined on", "Nonsense"]})
check("headers are guessed to fields, and what is left over is said", code == 200 and g["mapping"]["columns"].get("Amount") == "amount" and "Nonsense" in g["unmatched_headers"], (code, g))

# --- a dry run: what it would do, and nothing done
before = count(admin)
good = f"Name,Email,Amount,Status,Joined,Owner\nAnn,{mail('ann')},\"1,234.5\",won,2026-03-01,{OWN}\nBob,{mail('bob')},10,,01/03/2026,{OWN}\n"
bad = f"Cy,{mail('cy')},abc,new,2026-03-01,{OWN}\nDi,{mail('di')},5,pending,2026-03-01,{OWN}\n,{mail('ed')},5,new,2026-03-01,{OWN}\n"
file1 = good + bad
mapping = {"columns": {"Name": "name", "Email": "email", "Amount": "amount", "Status": "status", "Joined": "joined", "Owner": "owner"}, "key": ["email"], "notation": {"date_order": "dmy"}}
job, key1, status = run_import(admin, f"contacts-{tag}.csv", file1, "dry_run", mapping)
check("the dry run finishes", status.get("status") == "done", status)
check("it counts what a real run would do", (status["created"], status["updated"], status["unchanged"], status["errors"]) == (2, 0, 0, 3) and status["total_rows"] == 5 and status["processed"] == 5, status)
check("and has written nothing", count(admin) == before, (count(admin), before))
rows = rows_of(admin, job)
check("each row says what became of it", [r["action"] for r in rows] == ["create", "create", "error", "error", "error"] and rows[0]["row"] == 2, rows)
check("a bad cell is named with the column and what was expected", "`Amount`" in rows[2]["message"] and "not a number" in rows[2]["message"] and rows[3]["problems"][0]["value"] == "pending" and "new, won, lost" in rows[3]["message"], rows)
check("a row the model refuses says why in the model's words", "name" in rows[4]["message"].lower(), rows[4])
check("errors can be listed alone", [r["row"] for r in rows_of(admin, job, only_errors=True)] == [4, 5, 6])
check("the mapping used is kept", status["mapping_used"]["key"] == ["email"] and status["format"] == "csv", status)

# --- the real run, with its file of failed rows and its callback
job1, _, status = run_import(admin, f"contacts-{tag}.csv", file1, "run", mapping, ref=f"first-{tag}")
check("the real run finishes", status.get("status") == "done" and (status["created"], status["errors"]) == (2, 3), status)
check("it wrote the good rows", count(admin) == before + 2)
code, found = fn(admin, "find_contact", {"email": mail("ann")})
ann = found[0] if code == 200 and found else {}
check("with the values read as the file means them", ann.get("amount") == "1234.50" and ann.get("status") == "won" and ann.get("joined") == "2026-03-01" and ann.get("owner") == OWN, ann)
code, found = fn(admin, "find_contact", {"email": mail("bob")})
check("a day/month date and a blank choice (the model's default)", found[0].get("joined") == "2026-03-01" and found[0].get("status") == "new", found)
check("the rows that failed are in a file to fix", bool(status.get("errors_key")), status)
code, text = fn(admin, "read_file", {"key": status["errors_key"]})
lines = text["text"].lstrip("﻿").strip().splitlines()
check("the file has the original cells and a reason", lines[0].endswith("_error") and len(lines) == 4 and "abc" in lines[1], lines)
for _ in range(20):
    memos = fn(admin, "memos")[1]
    if any(f"import.done:done:{job1}:first-{tag}" in m["body"] for m in memos):
        break
    time.sleep(0.5)
check("the plugin that asked is told when it ends (on_done)", any(f"import.done:done:{job1}:first-{tag}" in m["body"] for m in memos), memos[-3:])

# --- the same file again
job2, _, status = run_import(admin, f"contacts-{tag}.csv", file1, "run", mapping)
check("the same file is not imported twice by accident", status.get("status") == "failed" and "already imported" in (status.get("error") or ""), status)
check("and nothing was written", count(admin) == before + 2)
job3, _, status = run_import(admin, f"contacts-{tag}.csv", file1, "run", mapping, force=True)
check("forced, it runs; with a key the rows that match change nothing", status["status"] == "done" and (status["created"], status["updated"], status["unchanged"], status["errors"]) == (0, 0, 2, 3), status)

# --- upsert: a changed cell is an update, a new key a create
file2 = f"Name,Email,Amount,Status,Joined,Owner\nAnn,{mail('ann')},2000,won,2026-03-01,{OWN}\nBob Marley,{mail('bob')},10,new,2026-03-01,{OWN}\nCy,{mail('cy2')},3,new,,{OWN}\n"
job4, _, status = run_import(admin, f"update-{tag}.csv", file2, "run", mapping)
check("a changed cell updates, an unchanged row does not, a new key creates", (status["created"], status["updated"], status["unchanged"]) == (1, 2, 0), status)
rows = rows_of(admin, job4)
check("an update keeps what it replaced", rows[0]["before"]["amount"] == "1234.50" and rows[0]["after"]["amount"] == "2000.00", rows[0])
code, found = fn(admin, "find_contact", {"email": mail("bob")})
check("the update is in the record", found[0]["name"] == "Bob Marley")

# --- undo
code, body = fn(admin, "import_undo", {"job": job4})
check("a finished real import can be undone", code == 200 and body.get("status") == "undoing", (code, body))
status = wait_for(admin, job4)
check("the undo finishes", status.get("status") == "undone" and status["undo"]["undone"] == 3, status)
code, found = fn(admin, "find_contact", {"email": mail("ann")})
check("changed records got their old values back", found[0]["amount"] == "1234.50")
code, found = fn(admin, "find_contact", {"email": mail("bob")})
check("including a name", found[0]["name"] == "Bob")
check("and the record it created is gone", fn(admin, "find_contact", {"email": mail("cy2")})[1] == [])
code, body = fn(admin, "import_undo", {"job": job4})
check("an import is undone once", code != 200, (code, body))
code, body = fn(admin, "import_undo", {"job": job})
check("a dry run has nothing to undo", code != 200, (code, body))

# a record changed after the import is left alone
file3 = f"Name,Email,Amount,Owner\nZed,{mail('zed')},1,{OWN}\n"
job5, _, status = run_import(admin, f"zed-{tag}.csv", file3, "run", {**mapping, "columns": {"Name": "name", "Email": "email", "Amount": "amount", "Owner": "owner"}})
zed = fn(admin, "find_contact", {"email": mail("zed")})[1][0]
fn(admin, "rename_contact", {"id": zed["id"], "name": "Zed, edited"})
fn(admin, "import_undo", {"job": job5})
status = wait_for(admin, job5)
check("a record changed since is left alone and the undo says so", status["status"] == "undone" and status["undo"]["skipped"] == 1, status)
check("and it is still there", fn(admin, "find_contact", {"email": mail("zed")})[1][0]["name"] == "Zed, edited")

# --- the importer is the person: an ordinary member's rules apply to every row
file4 = f"Name,Email,Owner\nMine,{mail('bob1')},{bob_id}\nTheirs,{mail('bob2')},{OWN}\n"
job6, _, status = run_import(bob, f"bobs-{tag}.csv", file4, "run", {"columns": {"Name": "name", "Email": "email", "Owner": "owner"}})
rows = rows_of(bob, job6)
check("an ordinary member imports only what their rules let them", [r["action"] for r in rows] == ["create", "error"] and status["errors"] == 1, (status, rows))
check("and is told in the rules' words", "not allowed" in rows[1]["message"].lower() or "may not" in rows[1]["message"].lower(), rows[1])
code, body = fn(admin, "import_status", {"job": job6})
check("an import is its starter's report, not anyone else's", code != 200, (code, body))
code, body = fn(bob, "import_undo", {"job": job1})
check("nor can another person undo it", code != 200, (code, body))

# --- stop on the first error, a bad mapping, a missing file
text = f"Name,Email,Owner\nOne,{mail('s1')},{OWN}\nTwo,{mail('s2')},{OWN}\n,{mail('s3')},{OWN}\nFour,{mail('s4')},{OWN}\n"
job7, _, status = run_import(admin, f"stop-{tag}.csv", text, "run", {"columns": {"Name": "name", "Email": "email", "Owner": "owner"}}, stop_on_error=True)
check("stop_on_error ends at the first row that fails", status["status"] == "done" and status["errors"] == 1 and status["created"] == 2 and status["processed"] == 3, status)
job8, _, status = run_import(admin, f"badmap-{tag}.csv", "a,b\n1,2\n", "dry_run", {"columns": {"a": "nosuchfield"}})
check("a mapping that cannot work fails the import and says why", status["status"] == "failed" and "not a field" in status["error"], status)
job9, _, status = run_import(admin, f"nomap-{tag}.csv", "color,size\nred,1\n", "dry_run")
check("a file with no column for a required field fails at once", status["status"] == "failed" and "no column" in status["error"] and "name" in status["guessed"]["missing_required"], status)
code, body = fn(admin, "import_start", {"media_key": "media/2026/01/nosuchfile.csv"})
check("a file that does not exist is refused when starting", code != 200, (code, body))
code, body = fn(admin, "import_start", {"media_key": "x", "storage_key": "y"})
check("naming two files is refused", code != 200, (code, body))
code, body = fn(admin, "import_start", {"media_key": "media/x", "mode": "sideways"})
check("an unknown mode is refused", code != 200, (code, body))
code, body = fn(admin, "import_start", {"media_key": "media/x", "mapping": {"colums": {}}})
check("a mapping with a misspelt key is refused", code != 200, (code, body))

# --- XLSX
sheet = xlsx([["Name", "Email", "Amount", "Owner"], ["Xia", mail("xia"), 12.5, OWN], ["Yan", mail("yan"), 7, OWN]])
job10, _, status = run_import(admin, f"book-{tag}.xlsx", sheet, "run", {"columns": {"Name": "name", "Email": "email", "Amount": "amount", "Owner": "owner"}}, ) if False else (None, None, None)
code, up = upload(admin, f"book-{tag}.xlsx", sheet, "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet")
check("a spreadsheet is uploaded", code == 200, (code, up))
code, started = fn(admin, "import_start", {"media_key": up["key"], "mode": "run"})
status = wait_for(admin, started["job"])
check("an XLSX is imported, its columns guessed from the headers", status["status"] == "done" and status["format"] == "xlsx" and (status["created"], status["errors"]) == (2, 0), status)
check("numbers become decimals", fn(admin, "find_contact", {"email": mail("xia")})[1][0]["amount"] == "12.50")

# --- a larger file: several batches
n = 3000
big = "Name,Email,Amount,Owner\n" + "".join(f"Person {i},{i}.{tag}@big.test,{i % 100}.5,{OWN}\n" for i in range(n))
started_at = time.time()
job11, _, status = run_import(admin, f"big-{tag}.csv", big, "run", {"columns": {"Name": "name", "Email": "email", "Amount": "amount", "Owner": "owner"}, "key": ["email"]})
took = time.time() - started_at
check(f"{n} rows are imported in batches ({took:.0f}s)", status["status"] == "done" and (status["created"], status["errors"], status["processed"]) == (n, 0, n), status)
job12, _, status = run_import(admin, f"big-{tag}.csv", big, "run", {"columns": {"Name": "name", "Email": "email", "Amount": "amount", "Owner": "owner"}, "key": ["email"]}, force=True)
check("and the same again changes nothing", (status["created"], status["updated"], status["unchanged"]) == (0, 0, n), status)

# --- export, and back in
code, out = fn(admin, "export_contacts", {"filter": {"owner": OWN}, "fields": ["name", "email", "amount", "status", "joined", "owner"]})
check("records are exported to a file in the plugin's storage", code == 200 and out["rows"] >= n and out["format"] == "csv", (code, out))
code, text = fn(admin, "read_file", {"key": out["storage_key"]})
check("a CSV with a header row and every record", text["text"].lstrip("﻿").splitlines()[0] == "name,email,amount,status,joined,owner" and len(text["text"].strip().splitlines()) == out["rows"] + 1)
job13, _, status = run_import(admin, f"roundtrip-{tag}.csv", text["text"], "run", {"columns": {k: k for k in ["name", "email", "amount", "status", "joined", "owner"]}, "key": ["email"]})
check("the export imports back, and changes nothing", status["status"] == "done" and (status["created"], status["updated"], status["errors"]) == (0, 0, 0), status)
code, out = fn(admin, "export_contacts", {"filter": {"email": mail("ann")}, "format": "json", "include_id": True})
code, text = fn(admin, "read_file", {"key": out["storage_key"]})
exported = json.loads(text["text"])
check("JSON is an array of records, with ids when asked", len(exported) == 1 and exported[0]["email"] == mail("ann") and exported[0]["id"], exported)
code, out = fn(bob, "export_contacts", {})
code, text = fn(bob, "read_file", {"key": out["storage_key"]})
check("an export follows the caller's rules: bob gets only his own", all(f",{bob_id}" in line for line in text["text"].strip().splitlines()[1:]) and out["rows"] >= 1, out)
code, body = fn(admin, "export_contacts", {"fields": ["nosuchfield"]})
check("an unknown field is refused", code != 200, (code, body))
code, body = fn(admin, "export_contacts", {"format": "pdf"})
check("an unknown format is refused", code != 200, (code, body))

finish()
