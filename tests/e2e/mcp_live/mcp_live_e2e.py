"""The live tools of `aether --mcp`, end to end, against a real SurrealDB: read-only mode, then write mode
(check, load, install, models, commands, run, upgrade). Talks to the binary the way an MCP client does.

Needs the instance from setup.sh. Plain Python 3, no packages. AETHER_BIN overrides the binary."""
import json, os, shutil, subprocess, sys, tempfile

here = os.path.dirname(os.path.abspath(__file__))
BIN = os.environ.get("AETHER_BIN", os.path.join(here, "../../../target/debug/aether"))
CONFIG = os.path.join(here, "aether.toml")
ORG = "mcp_org"
PLUGIN = os.path.join(here, "mcpplug")
failures = []

def check(name, ok, detail=""):
    print(("ok   " if ok else "FAIL ") + name + ("" if ok else f"  -> {detail}"))
    if not ok:
        failures.append(name)

class Mcp:
    def __init__(self, *extra):
        self.p = subprocess.Popen([BIN, "--mcp", "-c", CONFIG, "--org", ORG, *extra], stdin=subprocess.PIPE,
                                  stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, text=True, cwd=tempfile.gettempdir())
        self.n = 0
        self.send({"jsonrpc": "2.0", "id": self.next(), "method": "initialize", "params": {
            "protocolVersion": "2025-06-18", "capabilities": {}, "clientInfo": {"name": "e2e", "version": "0"}}})
        self.read()
        self.send({"jsonrpc": "2.0", "method": "notifications/initialized"})
    def next(self):
        self.n += 1
        return self.n
    def send(self, m):
        self.p.stdin.write(json.dumps(m) + "\n")
        self.p.stdin.flush()
    def read(self):
        while True:
            line = self.p.stdout.readline()
            if not line:
                raise RuntimeError("the server closed")
            m = json.loads(line)  # stdout must carry nothing but JSON-RPC
            if "id" in m:
                return m
    def call(self, name, **arguments):
        i = self.next()
        self.send({"jsonrpc": "2.0", "id": i, "method": "tools/call", "params": {"name": name, "arguments": arguments}})
        m = self.read()
        if "error" in m:
            return True, json.dumps(m["error"])
        r = m["result"]
        return bool(r.get("isError")), r["content"][0]["text"] if r.get("content") else ""
    def tools(self):
        i = self.next()
        self.send({"jsonrpc": "2.0", "id": i, "method": "tools/list"})
        return {t["name"] for t in self.read()["result"]["tools"]}
    def close(self):
        self.p.stdin.close()
        self.p.kill()
        self.p.wait()

READ = {"list_installed_plugins", "list_catalog_plugins", "get_org_models", "list_plugin_commands", "check_plugin_against_catalog"}
WRITE = {"load_plugin", "install_plugin", "upgrade_plugin", "run_command"}

# ---- read-only server ----
ro = Mcp()
names = ro.tools()
check("read-only: the read tools are offered", READ <= names, names)
check("read-only: the write tools are not", not (WRITE & names), WRITE & names)
err, text = ro.call("list_installed_plugins")
check("read-only: an empty organization says so", not err and "No plugin is installed" in text, text)
err, text = ro.call("list_catalog_plugins")
check("read-only: an empty catalog says so", not err and "no active plugin" in text, text)
err, text = ro.call("check_plugin_against_catalog", path=PLUGIN)
check("read-only: a plugin is checked against the catalog", not err and "VALID against the catalog: mcpplug 0.1.0" in text, text)
err, text = ro.call("load_plugin", path=PLUGIN)
check("read-only: load_plugin is refused", err, text)
ro.close()

# ---- write server ----
rw = Mcp("--mcp-allow-write")
check("write: every live tool is offered", (READ | WRITE) <= rw.tools())

err, text = rw.call("check_plugin_against_catalog", path=os.path.join(here, "no_such_plugin"))
check("a path that is not a plugin is an error", err, text)

err, text = rw.call("load_plugin", path=PLUGIN)
check("load_plugin: new", not err and "mcpplug 0.1.0 (new)" in text, text)
err, text = rw.call("load_plugin", path=PLUGIN)
check("load_plugin: the same files again are unchanged", not err and "mcpplug 0.1.0 (unchanged)" in text, text)
err, text = rw.call("list_catalog_plugins")
check("list_catalog_plugins shows it", not err and "mcpplug" in text and "MCP live test" in text, text)
err, text = rw.call("list_installed_plugins")
check("not installed yet", not err and "mcpplug" not in text, text)

err, text = rw.call("install_plugin", plugin="mcpplug")
check("install_plugin", not err and "Installed in `mcp_org`: mcpplug 0.1.0" in text, text)
check("install_plugin runs the on_install seed command", "Seed mcpplug.seed: ok" in text, text)
err, text = rw.call("install_plugin", plugin="mcpplug")
check("install again: already installed", not err and "Already installed: mcpplug 0.1.0" in text, text)
err, text = rw.call("list_installed_plugins")
check("list_installed_plugins", not err and "- mcpplug 0.1.0" in text, text)
err, text = rw.call("get_org_models", plugin="mcpplug")
check("get_org_models shows the model and its field", not err and "note" in text and "body" in text, text)
err, text = rw.call("list_plugin_commands")
check("list_plugin_commands shows the commands and arguments", not err and "mcpplug.greet" in text and "name: string" in text and "required" in text, text)

err, text = rw.call("run_command", command="mcpplug.greet", arguments={"name": "Ann", "times": 2})
out = {}
try:
    out = json.loads(text)
except ValueError:
    pass
check("run_command returns the plugin's answer", not err and out.get("greeting") == "hello Ann! hello Ann! ", text)
check("the seed note and the greeting are both there", out.get("notes") == 2, text)
err, text = rw.call("run_command", command="mcpplug.greet", arguments={"times": 2})
check("a missing required argument is refused", err and "name" in text, text)
err, text = rw.call("run_command", command="mcpplug.boom")
check("a failing command is an error that says why", err and "always fails" in text, text)
err, text = rw.call("run_command", command="greet")
check("a command without a plugin is refused", err and "<plugin>.<command>" in text, text)
err, text = rw.call("run_command", command="mcpplug.nope")
check("an unknown command is an error", err, text)

# A second version: same plugin, changed.
v2 = tempfile.mkdtemp(prefix="mcpplug_v2_")
try:
    shutil.copytree(PLUGIN, os.path.join(v2, "mcpplug"))
    manifest = os.path.join(v2, "mcpplug", "plugin.toml")
    with open(manifest) as f:
        text = f.read().replace('version = "0.1.0"', 'version = "0.2.0"')
    with open(manifest, "w") as f:
        f.write(text)
    err, text = rw.call("load_plugin", path=os.path.join(v2, "mcpplug"))
    check("load_plugin: a new version", not err and "mcpplug 0.2.0 (new)" in text, text)
    err, text = rw.call("upgrade_plugin", plugin="mcpplug")
    check("upgrade_plugin", not err and "mcpplug 0.1.0 -> 0.2.0" in text, text)
    err, text = rw.call("upgrade_plugin", plugin="mcpplug")
    check("upgrade again: already current", not err and "Already current: mcpplug 0.2.0" in text, text)
    err, text = rw.call("list_installed_plugins")
    check("the new version is installed", not err and "- mcpplug 0.2.0" in text, text)
finally:
    shutil.rmtree(v2, ignore_errors=True)
rw.close()

# ---- a second session sees what the first did (it is in the database, not in memory) ----
again = Mcp()
err, text = again.call("list_installed_plugins")
check("a new session sees the installed plugin", not err and "mcpplug 0.2.0" in text, text)
again.close()

print("\n%d failure(s)" % len(failures))
sys.exit(1 if failures else 0)
