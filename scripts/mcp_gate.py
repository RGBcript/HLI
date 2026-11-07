#!/usr/bin/env python3
"""Gate de HLI 0.3 con un cliente MCP externo real (JSON-RPC 2.0 sobre stdio).

No importa el crate: habla el protocolo con framing Content-Length, como un
cliente MCP cualquiera. Verifica además la trayectoria visible de rama, la
cobertura del prompt, la memoria en el manifiesto, el receipt finalizado,
la reconexión tras reiniciar el daemon y export/import.
"""
import json
import os
import shutil
import subprocess
import sys
import tempfile
import time

ROOT = r"C:\Users\GUERRA\Documents\TEC\HLI"
HLI = os.path.join(ROOT, "target", "debug", "hli.exe")
DAEMON = os.path.join(ROOT, "target", "debug", "hli-daemon.exe")
MCP = os.path.join(ROOT, "target", "debug", "hli-mcp.exe")


def run(env, *args):
    p = subprocess.run([HLI, *args], env=env, capture_output=True, text=True)
    if p.returncode != 0:
        raise RuntimeError(f"{args}: {p.stderr or p.stdout}")
    return json.loads(p.stdout) if p.stdout.strip().startswith(("{", "[")) else p.stdout


def start_daemon(env):
    proc = subprocess.Popen([DAEMON], env=env, stdin=subprocess.DEVNULL,
                            stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    for _ in range(100):
        time.sleep(0.05)
        p = subprocess.run([HLI, "daemon", "status"], env=env, capture_output=True, text=True)
        if p.returncode == 0 and '"running": true' in p.stdout:
            return proc
    raise RuntimeError("daemon no arrancó")


def stop_daemon(env, proc):
    subprocess.run([HLI, "daemon", "stop"], env=env, capture_output=True)
    try:
        proc.wait(timeout=5)
    except Exception:
        proc.kill()


class Mcp:
    def __init__(self, env):
        self.p = subprocess.Popen([MCP], env=env, stdin=subprocess.PIPE,
                                  stdout=subprocess.PIPE, stderr=subprocess.PIPE)
        self.n = 0

    def call(self, method, params=None):
        self.n += 1
        msg = json.dumps({"jsonrpc": "2.0", "id": self.n, "method": method,
                          "params": params or {}}).encode()
        self.p.stdin.write(f"Content-Length: {len(msg)}\r\n\r\n".encode() + msg)
        self.p.stdin.flush()
        headers = b""
        while not headers.endswith(b"\r\n\r\n"):
            ch = self.p.stdout.read(1)
            if not ch:
                raise RuntimeError("mcp cerró stdout")
            headers += ch
        length = 0
        for line in headers.decode().split("\r\n"):
            if line.lower().startswith("content-length:"):
                length = int(line.split(":")[1].strip())
        body = self.p.stdout.read(length)
        resp = json.loads(body)
        if "error" in resp:
            raise RuntimeError(f"{method}: {resp['error']}")
        return resp["result"]

    def close(self):
        try:
            self.p.stdin.close()
        except Exception:
            pass
        self.p.wait(timeout=5)


def main():
    base = tempfile.mkdtemp(prefix="hli-gate-03-")
    env = dict(os.environ, HLI_HOME=os.path.join(base, "home"))
    os.makedirs(env["HLI_HOME"], exist_ok=True)
    root = os.path.join(base, "root")
    os.makedirs(root, exist_ok=True)

    daemon = start_daemon(env)
    try:
        ws = run(env, "workspace", "create", "demo", "--root", root)["id"]
        th = run(env, "thread", "create", "research", "--workspace", ws)["id"]

        # --- trayectoria visible de rama ---
        base_ev = run(env, "event", "append", "--type", "base", "--payload", '{"n":0}',
                      "--workspace", ws, "--thread", th)
        a = run(env, "branch", "create", "A", "--thread", th, "--from", base_ev["event_id"])
        a_ev = run(env, "event", "append", "--type", "a", "--payload", '{"n":1}',
                   "--workspace", ws, "--thread", th, "--branch", a["id"])
        b = run(env, "branch", "create", "B", "--thread", th, "--from", a_ev["event_id"])
        b_ev = run(env, "event", "append", "--type", "b", "--payload", '{"n":2}',
                   "--workspace", ws, "--thread", th, "--branch", b["id"])
        c = run(env, "branch", "create", "C", "--thread", th, "--from", a_ev["event_id"])
        c_ev = run(env, "event", "append", "--type", "c", "--payload", '{"n":3}',
                   "--workspace", ws, "--thread", th, "--branch", c["id"])

        run_b = run(env, "run", "start", "--workspace", ws, "--thread", th,
                    "--branch", b["id"], "--prompt", "hola")
        got_b = run(env, "run", "get", run_b["id"])
        visible_b = [e["event_id"] for e in got_b["manifest"]["recent_events"]]
        assert a_ev["event_id"] in visible_b, "ancestor_event_is_included"
        assert b_ev["event_id"] in visible_b, "own_event_included"
        assert c_ev["event_id"] not in visible_b, "sibling_branch_event_is_excluded"

        run_a = run(env, "run", "start", "--workspace", ws, "--thread", th,
                    "--branch", a["id"], "--prompt", "hola")
        visible_a = [e["event_id"] for e in run(env, "run", "get", run_a["id"])["manifest"]["recent_events"]]
        assert b_ev["event_id"] not in visible_a, "descendant_event_is_excluded"

        # post_revision: nuevo evento en B después de crear el run
        run_b2 = run(env, "run", "start", "--workspace", ws, "--thread", th,
                     "--branch", b["id"], "--prompt", "hola")
        late = run(env, "event", "append", "--type", "late", "--payload", '{"n":4}',
                   "--workspace", ws, "--thread", th, "--branch", b["id"])
        visible_b2 = [e["event_id"] for e in run(env, "run", "get", run_b2["id"])["manifest"]["recent_events"]]
        assert late["event_id"] not in visible_b2, "post_revision_event_is_excluded"

        # --- prompt y memoria en el manifiesto ---
        mem = run(env, "memory", "commit", "--workspace", ws, "--type", "decision",
                  "--content", "SQLite es la fuente de verdad")
        assert mem["status"] == "active"
        found = run(env, "memory", "search", "--workspace", ws, "--query", "SQLite")
        assert any(m["id"] == mem["id"] for m in found), "memory_search"

        run_m = run(env, "run", "start", "--workspace", ws, "--thread", th,
                    "--branch", b["id"], "--prompt", "SQLite para persistir")
        got_m = run(env, "run", "get", run_m["id"])
        assert got_m["manifest"]["user_input"]["content"] == "SQLite para persistir"
        assert got_m["manifest"]["user_input"]["content_hash"].startswith("sha256:")
        assert any(m["id"] == mem["id"] for m in got_m["manifest"]["memories"]), "memory_in_manifest"
        receipt = got_m["run"]["receipt"]
        assert receipt["terminal_event"], "receipt_terminal_event"
        assert receipt["result_hash"].startswith("sha256:")
        assert receipt["context_manifest"] == got_m["manifest"]["manifest_id"]

        # --- cliente MCP externo ---
        mcp = Mcp(env)
        init = mcp.call("initialize", {"protocolVersion": "2025-11-25", "capabilities": {},
                                       "clientInfo": {"name": "gate", "version": "1"}})
        assert init["serverInfo"]["name"] == "hli"
        tools = [t["name"] for t in mcp.call("tools/list")["tools"]]
        for t in ["workspace_list", "thread_read", "memory_search", "memory_commit",
                  "run_start", "run_get", "artifact_read"]:
            assert t in tools, f"tool {t}"
        assert mcp.call("tools/call", {"name": "workspace_list", "arguments": {}})
        m2 = mcp.call("tools/call", {"name": "memory_commit", "arguments": {
            "workspace_id": ws, "type": "fact", "content": "MCP funciona"}})
        assert m2["isError"] is False
        res = mcp.call("tools/call", {"name": "memory_search",
                                      "arguments": {"workspace_id": ws, "query": "MCP"}})
        assert "MCP funciona" in res["content"][0]["text"]
        rr = mcp.call("resources/read", {"uri": f"hli://run/{run_m['id']}"})
        assert run_m["id"] in rr["contents"][0]["text"]
        mcp.close()

        # --- export/import ---
        archive = os.path.join(base, "ws.hliar")
        run(env, "export", ws, "--out", archive)
        assert os.path.exists(archive)
        stop_daemon(env, daemon)

        env2 = dict(os.environ, HLI_HOME=os.path.join(base, "home2"))
        os.makedirs(env2["HLI_HOME"], exist_ok=True)
        daemon2 = start_daemon(env2)
        run(env2, "import", archive)
        found2 = run(env2, "memory", "search", "--workspace", ws, "--query", "SQLite")
        assert any(m["id"] == mem["id"] for m in found2), "memory_survives_import"
        got3 = run(env2, "run", "get", run_m["id"])
        assert got3["run"]["state"] == "completed"
        assert got3["manifest"]["content_hash"] == got_m["manifest"]["content_hash"]
        stop_daemon(env2, daemon2)

        print(json.dumps({"gate": "0.3", "ok": True,
                          "tools": tools, "memories": len(found2),
                          "run": run_m["id"], "manifest": got_m["manifest"]["manifest_id"]}))
    finally:
        try:
            stop_daemon(env, daemon)
        except Exception:
            pass
        shutil.rmtree(base, ignore_errors=True)


if __name__ == "__main__":
    main()
