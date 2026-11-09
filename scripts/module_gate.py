#!/usr/bin/env python3
"""Gate 0.4: empaqueta hli-memory.hlimod, instala, prueba, crea MCH, expone por
CLI y MCP, reinicia, exporta/importa y verifica checksums, SDK, permisos y vigencia.
"""
import hashlib
import json
import os
import shutil
import struct
import subprocess
import sys
import tempfile
import time

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
TARGET = os.path.join(ROOT, "target", "debug")
HLI = os.path.join(TARGET, "hli.exe")
DAEMON = os.path.join(TARGET, "hli-daemon.exe")
MCP = os.path.join(TARGET, "hli-mcp.exe")
def _binary(name: str, package: str) -> str:
    base = os.path.join(TARGET, name)
    for candidate in (base + ".exe", base):
        if os.path.exists(candidate):
            return candidate
    subprocess.run(["cargo", "build", "-p", package], cwd=ROOT, check=True)
    for candidate in (base + ".exe", base):
        if os.path.exists(candidate):
            return candidate
    raise SystemExit(f"no se encontró el binario {name}")


MODULE = _binary("hli-memory", "hli-module-memory")


def sha(b):
    return "sha256:" + hashlib.sha256(b).hexdigest()


def container(files):
    out = b"HLIAR1\n"
    for path, data in files:
        out += f"FILE {len(data)} {path}\n".encode() + data + b"\n"
    return out


def build_package(dest, sdk_override=None, tamper=False):
    files = []
    manifest = json.load(open(os.path.join(ROOT, "modules", "hli-memory", "manifest.json"), encoding="utf-8"))
    if sdk_override:
        manifest["spec"]["sdk"] = sdk_override
    files.append(("manifest.json", json.dumps(manifest).encode()))
    schemas = os.path.join(ROOT, "modules", "hli-memory", "schemas")
    for name in sorted(os.listdir(schemas)):
        files.append((f"schemas/{name}", open(os.path.join(schemas, name), "rb").read()))
    binary_orig = open(MODULE, "rb").read()
    checks = {p: sha(d) for p, d in files}
    checks["bin/hli-memory.exe"] = sha(binary_orig)
    binary = binary_orig
    if tamper:
        b = bytearray(binary_orig)
        b[0] ^= 0xFF
        binary = bytes(b)
    files.append(("bin/hli-memory.exe", binary))
    files.append(("checksums.json", json.dumps(checks).encode()))
    open(dest, "wb").write(container(files))
    return dest


def run(env, *args, check=True):
    p = subprocess.run([HLI, *args], env=env, capture_output=True, text=True)
    if check and p.returncode != 0:
        raise RuntimeError(f"{args}: {p.stderr or p.stdout}")
    return json.loads(p.stdout) if p.stdout.strip().startswith(("{", "[")) else p.stdout


def start(env):
    proc = subprocess.Popen([DAEMON], env=env, stdin=subprocess.DEVNULL,
                            stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    for _ in range(100):
        time.sleep(0.05)
        p = subprocess.run([HLI, "daemon", "status"], env=env, capture_output=True, text=True)
        if p.returncode == 0 and '"running": true' in p.stdout:
            return proc
    raise RuntimeError("daemon no arrancó")


def stop(env, proc):
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


def main():
    base = tempfile.mkdtemp(prefix="hli-gate-04-")
    env = dict(os.environ, HLI_HOME=os.path.join(base, "home"))
    os.makedirs(env["HLI_HOME"], exist_ok=True)
    pkg = os.path.join(base, "hli-memory-0.9.0-beta.1.hlimod")
    build_package(pkg)

    d = start(env)
    try:
        ws = run(env, "workspace", "create", "demo", "--root", base)["id"]
        out = run(env, "module", "install", pkg)
        assert out["installed"] == "hli-memory", out
        test = run(env, "module", "test", "hli-memory")
        assert test["ok"] is True

        mch = run(env, "mch", "create", "memory-main", "--module", "hli-memory",
                  "--domain", "memory", "--workspace", ws)
        assert mch["profile"] == "memory-main"
        served = run(env, "mch", "serve", "memory-main")
        assert served["instance"]["state"] == "ready"

        op = run(env, "mch", "invoke", "memory-main", "memory.commit",
                 "--args", '{"type":"fact","content":"MCH Memory funciona"}',
                 "--workspace", ws)
        assert op["result"]["status"] == "active"
        found = run(env, "mch", "invoke", "memory-main", "memory.search",
                    "--args", '{"query":"MCH"}', "--workspace", ws)
        assert any("MCH Memory" in m["content"] for m in found["result"])

        mcp = Mcp(env)
        init = mcp.call("initialize", {"protocolVersion": "2025-11-25"})
        assert init["result"]["serverInfo"]["name"] == "hli"
        tools = [t["name"] for t in mcp.call("tools/list")["result"]["tools"]]
        assert "mch_invoke" in tools and "mch_list" in tools
        mc = mcp.call("tools/call", {"name": "mch_invoke", "arguments": {
            "name": "memory-main", "operation": "memory.commit",
            "arguments": {"type": "decision", "content": "MCP y CLI comparten esquema"}}})
        assert mc["result"]["isError"] is False
        ms = mcp.call("tools/call", {"name": "mch_invoke", "arguments": {
            "name": "memory-main", "operation": "memory.search",
            "arguments": {"query": "esquema"}}})
        assert "comparten esquema" in ms["result"]["content"][0]["text"]
        bad = mcp.call("tools/call", {"name": "mch_invoke", "arguments": {
            "name": "memory-main", "operation": "memory.drop"}})
        assert "error" in bad, "operación no declarada debía fallar"
        mcp.close()

        # vigencia
        mem = run(env, "memory", "commit", "--workspace", ws, "--type", "fact",
                  "--content", "memoria caducada")
        run(env, "memory", "update", mem["id"], "--valid-until", "2000-01-01T00:00:00.000Z")
        expired = run(env, "memory", "search", "--workspace", ws, "--query", "caducada")
        assert not any(m["id"] == mem["id"] for m in expired), "valid_until no aplica"

        # checksum manipulado
        badpkg = os.path.join(base, "tampered.hlimod")
        build_package(badpkg, tamper=True)
        r = subprocess.run([HLI, "module", "install", badpkg], env=env, capture_output=True, text=True)
        assert r.returncode != 0 and "checksum" in (r.stderr + r.stdout).lower()

        # SDK incompatible
        badsdk = os.path.join(base, "badsdk.hlimod")
        build_package(badsdk, sdk_override=">=9.0 <9.1")
        r = subprocess.run([HLI, "module", "install", badsdk], env=env, capture_output=True, text=True)
        assert r.returncode != 0 and "sdk" in (r.stderr + r.stdout).lower()

        # reinicio
        stop(env, d)
        d = start(env)
        assert run(env, "module", "list")
        # El proceso MCH no sobrevive al reinicio del daemon: queda stopped.
        assert run(env, "mch", "list")[0]["state"] == "stopped"
        run(env, "mch", "serve", "memory-main")
        assert run(env, "mch", "list")[0]["state"] == "ready"
        again = run(env, "memory", "search", "--workspace", ws, "--query", "MCH")
        assert again, "memoria perdida tras reinicio"

        # export/import
        archive = os.path.join(base, "ws.hliar")
        run(env, "export", ws, "--out", archive)
        stop(env, d)
        env2 = dict(os.environ, HLI_HOME=os.path.join(base, "home2"))
        os.makedirs(env2["HLI_HOME"], exist_ok=True)
        d2 = start(env2)
        run(env2, "import", archive)
        imported = run(env2, "memory", "search", "--workspace", ws, "--query", "MCH")
        assert imported, "memoria no sobrevive al import"
        stop(env2, d2)

        print(json.dumps({"gate": "0.4", "ok": True, "module": "hli-memory",
                          "instance": "memory-main", "mcps": ["mch_invoke", "mch_list"],
                          "memories": len(imported)}))
    finally:
        try:
            stop(env, d)
        except Exception:
            pass
        shutil.rmtree(base, ignore_errors=True)


if __name__ == "__main__":
    main()
