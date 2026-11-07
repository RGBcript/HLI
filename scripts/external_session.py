#!/usr/bin/env python3
"""Verificación de continuidad externa: dos sesiones MCP y cambio de modelo.

No es un harness anterior: un harness anterior 0.2.1-rc.5 no expone configuración MCP en esta instalación.
El cliente habla MCP real por stdio (Content-Length), en procesos separados.
"""
import json
import os
import shutil
import subprocess
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


class Session:
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
        h = b""
        while not h.endswith(b"\r\n\r\n"):
            c = self.p.stdout.read(1)
            if not c:
                raise RuntimeError("mcp cerró")
            h += c
        n = 0
        for line in h.decode().split("\r\n"):
            if line.lower().startswith("content-length:"):
                n = int(line.split(":")[1])
        return json.loads(self.p.stdout.read(n))

    def close(self):
        try:
            self.p.stdin.close()
        except Exception:
            pass
        self.p.wait(timeout=5)


def start_daemon(env):
    proc = subprocess.Popen([DAEMON], env=env, stdin=subprocess.DEVNULL,
                            stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    for _ in range(100):
        time.sleep(0.05)
        if subprocess.run([HLI, "daemon", "status"], env=env, capture_output=True).returncode == 0:
            return proc
    raise RuntimeError("daemon no arrancó")


def main():
    base = tempfile.mkdtemp(prefix="hli-ext-")
    env = dict(os.environ, HLI_HOME=os.path.join(base, "home"))
    os.makedirs(env["HLI_HOME"], exist_ok=True)
    open(os.path.join(env["HLI_HOME"], "prices.yaml"), "w").write(
        "provider: ollama\nmodel: gemma4:31b\ncurrency: USD\ninput_per_million: 0.27\n"
        "output_per_million: 1.10\neffective_from: 2026-11-07\nsource: user-configured\n")
    d = start_daemon(env)
    evidence = {}
    try:
        ws = run(env, "workspace", "create", "ext", "--root", base)["id"]
        th = run(env, "thread", "create", "ext", "--workspace", ws)["id"]
        br = run(env, "branch", "create", "main", "--thread", th)["id"]

        # Sesión A: registra una decisión por MCP y se cierra.
        a = Session(env)
        a.call("initialize", {"protocolVersion": "2025-11-25"})
        tools = [t["name"] for t in a.call("tools/list")["result"]["tools"]]
        a.call("tools/call", {"name": "memory_commit", "arguments": {
            "workspace_id": ws, "type": "decision",
            "content": "La decisión externa es usar SQLite WAL."}})
        a.close()
        evidence["session_a_tools"] = tools

        # Sesión B: recupera la decisión y lanza un run.
        b = Session(env)
        b.call("initialize", {"protocolVersion": "2025-11-25"})
        found = b.call("tools/call", {"name": "memory_search", "arguments": {
            "workspace_id": ws, "query": "decisión externa"}})
        b.close()
        evidence["session_b_found"] = "SQLite WAL" in found["result"]["content"][0]["text"]

        # Artifact + slice, run con evidencia.
        log = os.path.join(base, "log.txt")
        open(log, "w", encoding="utf-8").write("info\nERROR db timeout pool=8\nfin\n")
        aid = run(env, "artifact", "put", log, "--workspace", ws)["id"]
        run(env, "artifact", "slice", aid, "--workspace", ws, "--kind", "line_range",
            "--start", "1", "--end", "2", "--reason", "error line")
        rv = run(env, "run", "start", "--workspace", ws, "--thread", th, "--branch", br,
                 "--prompt", "¿Qué error aparece?", "--artifact", aid)
        got = run(env, "run", "get", rv["id"])
        evidence["run_slices"] = len(got["manifest"]["artifact_slices"])
        evidence["run_payload_hash"] = got["manifest"]["payload_hash"][:24]
        evidence["receipt_usage"] = got["run"]["receipt"]["usage"]["input_tokens"]

        # Cambio de modelo en el mismo thread.
        run(env, "run", "start", "--workspace", ws, "--thread", th, "--branch", br,
            "--prompt", "Responde solo: OK", "--provider", "ollama", "--model", "gemma4:31b-cloud",
            "--max-tokens", "16")
        evs = run(env, "event", "list", "--workspace", ws)
        evidence["handoffs"] = [e["payload"] for e in evs if e["type"] == "provider.handoff"]

        # Reinicio y sesión C.
        subprocess.run([HLI, "daemon", "stop"], env=env, capture_output=True)
        d.wait(timeout=5)
        d = start_daemon(env)
        c = Session(env)
        c.call("initialize", {"protocolVersion": "2025-11-25"})
        found2 = c.call("tools/call", {"name": "memory_search", "arguments": {
            "workspace_id": ws, "query": "SQLite WAL"}})
        run2 = c.call("tools/call", {"name": "run_get", "arguments": {"run_id": rv["id"]}})
        c.close()
        evidence["session_c_found"] = "SQLite WAL" in found2["result"]["content"][0]["text"]
        evidence["run_survives_restart"] = run2["result"]["content"][0]["text"].find(rv["id"]) >= 0

        evidence["ok"] = all([
            evidence["session_b_found"], evidence["session_c_found"],
            evidence["run_survives_restart"], evidence["run_slices"] >= 1,
            len(evidence["handoffs"]) >= 1,
        ])
        print(json.dumps(evidence))
    finally:
        subprocess.run([HLI, "daemon", "stop"], env=env, capture_output=True)
        try:
            d.wait(timeout=5)
        except Exception:
            d.kill()
        shutil.rmtree(base, ignore_errors=True)


if __name__ == "__main__":
    main()
