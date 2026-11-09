#!/usr/bin/env python3
"""Gate de planificación durable y aprobación previa (HLI 0.9).

Recorre el ciclo completo contra el daemon real: read-only sin aprobación,
write en espera sin ejecutar, aprobación desde una segunda CLI, rechazo,
reinicio del daemon con un run en espera, JSONL en vivo, MCP y
export/import de las revisiones.
"""
from __future__ import annotations

import json
import os
import shutil
import subprocess
import sys
import tempfile
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
HLI = ROOT / "target" / "debug" / "hli.exe"
DAEMON = ROOT / "target" / "debug" / "hli-daemon.exe"
MCP = ROOT / "target" / "debug" / "hli-mcp.exe"

RESULT = {
    "gate": "planning-approval",
    "ok": False,
    "plan_durable": False,
    "read_only_policy": False,
    "write_waits": False,
    "no_execution_before_approval": False,
    "approval_once": False,
    "approval_run": False,
    "approval_workspace": False,
    "tui_state": False,
    "external_approval": False,
    "rejection": False,
    "replan": False,
    "revision_history": False,
    "restart": False,
    "receipt": False,
    "secret_safety": False,
    "inspector_actions": False,
    "jsonl": False,
    "mcp": False,
    "export_import": False,
}

HOME = Path(tempfile.mkdtemp(prefix="hli-plan-gate-"))
PROJECT = Path(tempfile.mkdtemp(prefix="hli-plan-project-"))
ENV = {**os.environ, "HLI_HOME": str(HOME)}


def run(*args, cwd=PROJECT, timeout=120, env=None):
    return subprocess.run(
        [str(HLI), *args],
        capture_output=True,
        text=True,
        env=env or ENV,
        cwd=str(cwd),
        timeout=timeout,
    )


def jrun(*args, cwd=PROJECT, env=None):
    p = run(*args, "--output", "json", cwd=cwd, env=env)
    if p.returncode != 0:
        raise RuntimeError(f"{args}: {p.stderr or p.stdout}")
    return json.loads(p.stdout)


def start_daemon(env):
    proc = subprocess.Popen(
        [str(DAEMON)],
        env=env,
        stdin=subprocess.DEVNULL,
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
    )
    for _ in range(200):
        time.sleep(0.05)
        p = subprocess.run(
            [str(HLI), "daemon", "status"], env=env, capture_output=True, text=True
        )
        if p.returncode == 0 and '"running":true' in p.stdout.replace(" ", ""):
            return proc
    raise RuntimeError("el daemon no arrancó")


def stop_daemon(env, proc):
    subprocess.run([str(HLI), "daemon", "stop"], env=env, capture_output=True)
    try:
        proc.wait(timeout=5)
    except Exception:
        proc.kill()


def wait_state(env, run_id, state, timeout=10):
    deadline = time.time() + timeout
    last = None
    while time.time() < deadline:
        last = jrun("run", "status", run_id, env=env)["run"]
        if last["state"] == state:
            return last
        if last["state"] in ("failed", "cancelled", "timed_out", "interrupted"):
            return last
        time.sleep(0.1)
    return last


def events(ws):
    return jrun("event", "list", "--workspace", ws)


class Mcp:
    def __init__(self, env):
        self.p = subprocess.Popen(
            [str(MCP)],
            env=env,
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
        )
        self.n = 0

    def call(self, method, params=None):
        self.n += 1
        msg = json.dumps(
            {"jsonrpc": "2.0", "id": self.n, "method": method, "params": params or {}}
        ).encode()
        self.p.stdin.write(f"Content-Length: {len(msg)}\r\n\r\n".encode() + msg)
        self.p.stdin.flush()
        headers = {}
        while True:
            line = self.p.stdout.readline().decode().strip()
            if not line:
                break
            k, _, v = line.partition(":")
            headers[k.strip().lower()] = v.strip()
        body = self.p.stdout.read(int(headers["content-length"]))
        return json.loads(body)

    def close(self):
        self.p.kill()


try:
    daemon = start_daemon(ENV)
    ws = jrun("workspace", "create", "plan", "--root", str(PROJECT))["id"]
    th = jrun("thread", "create", "plan", "--workspace", ws)["id"]
    br = jrun("branch", "create", "main", "--thread", th)["id"]

    # 1) Read-only: política efectiva, sin aprobación.
    read_only = jrun(
        "run", "start", "--workspace", ws, "--thread", th, "--branch", br,
        "--prompt", "resume el hilo", "--provider", "sim",
    )
    RESULT["read_only_policy"] = read_only["state"] == "completed"
    plan_ro = jrun("run", "plan", read_only["id"])
    RESULT["plan_durable"] = (
        plan_ro["plan"]["origin"] == "runtime_generated"
        and plan_ro["plan"]["status"] == "completed"
        and plan_ro["plan"]["revision"] == 1
        and plan_ro["plan"]["content_hash"].startswith("sha256:")
    )
    RESULT["receipt"] = (
        read_only["receipt"]["plan_id"] == plan_ro["plan"]["plan_id"]
        and read_only["receipt"]["plan_hash"] == plan_ro["plan"]["content_hash"]
    )

    # 2) Write: espera y no ejecuta.
    target = PROJECT / "plan.txt"
    before = target.exists()
    write_run = jrun(
        "run", "start", "--workspace", ws, "--thread", th, "--branch", br,
        "--prompt", "tool:write plan.txt contenido", "--provider", "sim",
    )
    RESULT["write_waits"] = write_run["state"] == "waiting_approval"
    RESULT["no_execution_before_approval"] = before == target.exists()
    kinds = [e["type"] for e in events(ws) if e.get("run_id") == write_run["id"]]
    RESULT["plan_durable"] = RESULT["plan_durable"] and all(
        k in kinds for k in ("plan.created", "run.planning", "run.waiting_approval")
    )

    # 3) Aprobación desde una segunda CLI, fijando la revisión mostrada.
    shown = jrun("run", "plan", write_run["id"])
    approved = jrun(
        "run", "approve", write_run["id"], "--scope", "once",
        "--plan", shown["plan"]["plan_id"],
    )
    done = wait_state(ENV, write_run["id"], "completed")
    RESULT["external_approval"] = (
        approved["scope"] == "once"
        and done["state"] == "completed"
        and target.read_text(encoding="utf-8") == "contenido"
    )
    RESULT["receipt"] = RESULT["receipt"] and (
        done["receipt"]["approval_scope"] == "once"
        and done["receipt"]["approval_status"] == "completed"
        and done["receipt"]["plan_id"] == shown["plan"]["plan_id"]
    )
    RESULT["approval_once"] = done["state"] == "completed"
    # La TUI consume el mismo payload: plan, revisión, pasos y hash de snapshot.
    RESULT["tui_state"] = bool(
        shown["plan"]["revision"] >= 1
        and shown["execution"]["content_hash"].startswith("sha256:")
        and isinstance(shown["plan"]["steps"], list)
        and shown["plan"]["requested_effects"]
    )

    # 3b) scopes con un workspace aparte: el grant de workspace no debe
    # contaminar los runs de reinicio/rechazo del workspace principal.
    scope_root = PROJECT / "scopes"
    scope_root.mkdir(exist_ok=True)
    ws2 = jrun("workspace", "create", "scopes", "--root", str(scope_root))["id"]
    th2 = jrun("thread", "create", "scopes", "--workspace", ws2)["id"]
    br2 = jrun("branch", "create", "main", "--thread", th2)["id"]

    run_scope = jrun("run", "start", "--workspace", ws2, "--thread", th2, "--branch", br2,
                     "--prompt", "tool:write run.txt r", "--provider", "sim")
    jrun("run", "approve", run_scope["id"], "--scope", "run")
    RESULT["approval_run"] = wait_state(ENV, run_scope["id"], "completed")["state"] == "completed"

    ws_scope = jrun("run", "start", "--workspace", ws2, "--thread", th2, "--branch", br2,
                    "--prompt", "tool:write ws.txt w", "--provider", "sim")
    jrun("run", "approve", ws_scope["id"], "--scope", "workspace")
    persisted = wait_state(ENV, ws_scope["id"], "completed")["state"] == "completed"
    after_grant = jrun("run", "start", "--workspace", ws2, "--thread", th2, "--branch", br2,
                       "--prompt", "tool:write ws2.txt w2", "--provider", "sim")
    RESULT["approval_workspace"] = bool(
        persisted
        and after_grant["state"] == "completed"
        and (scope_root / "ws.txt").exists()
    )

    # 4) Rechazo: nunca ejecuta.
    reject_target = PROJECT / "reject.txt"
    reject_run = jrun(
        "run", "start", "--workspace", ws, "--thread", th, "--branch", br,
        "--prompt", "tool:write reject.txt no", "--provider", "sim",
    )
    rejected = jrun("run", "reject", reject_run["id"], "--reason", "no autorizado")
    RESULT["rejection"] = bool(
        rejected["run"]["state"] == "cancelled"
        and rejected["plan"]["status"] == "rejected"
        and rejected["terminal_reason"] == "approval_rejected"
        and not reject_target.exists()
        and rejected["run"]["receipt"]
        and rejected["run"]["receipt"]["result_hash"]
        and rejected["run"]["receipt"]["approval_status"] == "rejected"
    )

    # 4b) Replan tras un cambio material (modelo del proveedor).
    jrun("provider", "add", "local", "--kind", "ollama",
         "--base-url", "http://127.0.0.1:9", "--model", "m1", "--timeout", "2")
    rp = jrun("run", "start", "--workspace", ws, "--thread", th, "--branch", br,
              "--prompt", "tool:write replan.txt r", "--provider", "local")
    v1 = jrun("run", "plan", rp["id"])["plan"]
    jrun("provider", "add", "local", "--kind", "ollama",
         "--base-url", "http://127.0.0.1:9", "--model", "m2", "--timeout", "2")
    stale = run("run", "approve", rp["id"], "--scope", "once")
    jrun("run", "replan", rp["id"], "--reason", "model changed")
    after = jrun("run", "plan", rp["id"])
    revisions = after["revisions"]
    RESULT["replan"] = bool(
        stale.returncode != 0
        and "plan_stale" in (stale.stderr + stale.stdout)
        and after["plan"]["revision"] == 2
        and after["plan"]["supersedes_plan_id"] == v1["plan_id"]
        and after["plan"]["invalidation_reasons"]
        and after["run"]["state"] == "waiting_approval"
        and after["plan"]["status"] == "waiting_approval"
    )
    RESULT["revision_history"] = bool(
        len(revisions) == 2
        and revisions[0]["plan_id"] == v1["plan_id"]
        and revisions[1]["revision"] == 2
        and revisions[0]["content_hash"] == v1["content_hash"]
    )
    # Aprobar la revisión nueva sí funciona (el provider no responde, pero la
    # ejecución usa exactamente la instantánea aprobada).
    jrun("run", "approve", rp["id"], "--scope", "once", "--plan", after["plan"]["plan_id"])
    final = wait_state(ENV, rp["id"], "completed", timeout=10)
    receipt = final.get("receipt") or {}
    RESULT["receipt"] = RESULT["receipt"] and bool(
        receipt.get("plan_revision") == 2
        and receipt.get("superseded_plan_ids")
        and receipt.get("invalidation_events")
        and receipt.get("execution_snapshot_hash")
    )

    # 4c) Seguridad de secretos: ni SQLite, ni eventos, ni receipt, ni export.
    secret_value = "GATESECRET-77c1"
    secrets = HOME / "secrets"
    secrets.mkdir(exist_ok=True)
    (secrets / "gate").write_text(secret_value, encoding="utf-8")
    jrun("provider", "add", "gate-oai", "--kind", "openai",
         "--base-url", "http://127.0.0.1:9/v1", "--model", "m",
         "--timeout", "2", "--secret-handle", "gate")
    secret_run = jrun("run", "start", "--workspace", ws, "--thread", th, "--branch", br,
                      "--prompt", "tool:write secret.txt s", "--provider", "gate-oai")
    secret_plan = jrun("run", "plan", secret_run["id"])
    handles = secret_plan["execution"]["secret_handles"]
    jrun("run", "approve", secret_run["id"], "--scope", "once",
         "--plan", secret_plan["plan"]["plan_id"])
    wait_state(ENV, secret_run["id"], "failed", timeout=10)
    secret_export = HOME / "secret-check.hliar"
    jrun("export", ws, "--out", str(secret_export))
    blobs = [
        (HOME / "hli.db").read_bytes(),
        secret_export.read_bytes(),
        json.dumps(jrun("run", "get", secret_run["id"])).encode(),
        json.dumps(jrun("event", "list", "--workspace", ws)).encode(),
        json.dumps(secret_plan).encode(),
    ]
    RESULT["secret_safety"] = bool(
        handles
        and handles[0]["id"] == "gate"
        and handles[0]["version"].startswith("v")
        and all(secret_value.encode() not in b for b in blobs)
        and b"secret_values" not in blobs[1]
    )

    # 4d) Acciones del Inspector: mismas operaciones del daemon.
    insp = jrun("run", "start", "--workspace", ws, "--thread", th, "--branch", br,
                "--prompt", "tool:write inspect.txt i", "--provider", "sim")
    insp_plan = jrun("run", "plan", insp["id"])["plan"]
    once = jrun("run", "approve", insp["id"], "--scope", "once", "--plan", insp_plan["plan_id"])
    insp2 = jrun("run", "start", "--workspace", ws, "--thread", th, "--branch", br,
                 "--prompt", "tool:write inspect2.txt i", "--provider", "sim")
    rej = jrun("run", "reject", insp2["id"], "--plan",
               jrun("run", "plan", insp2["id"])["plan"]["plan_id"],
               "--reason", "rejected_from_inspector")
    insp3 = jrun("run", "start", "--workspace", ws, "--thread", th, "--branch", br,
                 "--prompt", "tool:write inspect3.txt i", "--provider", "sim")
    v1_3 = jrun("run", "plan", insp3["id"])["plan"]["plan_id"]
    jrun("provider", "add", "local", "--kind", "ollama",
         "--base-url", "http://127.0.0.1:9", "--model", "m3", "--timeout", "2")
    rp3 = jrun("run", "start", "--workspace", ws, "--thread", th, "--branch", br,
               "--prompt", "tool:write inspect4.txt i", "--provider", "local")
    v1_4 = jrun("run", "plan", rp3["id"])["plan"]["plan_id"]
    jrun("provider", "add", "local", "--kind", "ollama",
         "--base-url", "http://127.0.0.1:9", "--model", "m4", "--timeout", "2")
    stale_inspector = run("run", "approve", rp3["id"], "--plan", v1_4)
    replanned = jrun("run", "replan", rp3["id"], "--plan", v1_4,
                     "--reason", "provider changed")
    RESULT["inspector_actions"] = bool(
        once["run"]["state"] == "running"
        and rej["run"]["state"] == "cancelled"
        and rej["plan"]["status"] == "rejected"
        and stale_inspector.returncode != 0
        and replanned["plan"]["revision"] == 2
        and replanned["plan"]["supersedes_plan_id"] == v1_4
    )

    # 5) Reinicio con un run en espera: el plan y la instantánea sobreviven.
    pending = jrun(
        "run", "start", "--workspace", ws, "--thread", th, "--branch", br,
        "--prompt", "tool:write restart.txt ok", "--provider", "sim",
    )
    stop_daemon(ENV, daemon)
    daemon = start_daemon(ENV)
    after = jrun("run", "status", pending["id"])["run"]
    plan_after = jrun("run", "plan", pending["id"])
    jrun("run", "approve", pending["id"], "--scope", "run")
    resumed = wait_state(ENV, pending["id"], "completed")
    RESULT["restart"] = (
        after["state"] == "waiting_approval"
        and plan_after["plan"]["revision"] == 1
        and resumed["state"] == "completed"
        and (PROJECT / "restart.txt").exists()
    )

    # 6) JSONL en vivo: waiting → aprobación externa → continuación.
    follow = subprocess.Popen(
        [str(HLI), "run", "tool:write follow.txt vivo", "--output", "jsonl",
         "--follow", "--timeout-secs", "30"],
        env=ENV, cwd=str(PROJECT), stdout=subprocess.PIPE, stderr=subprocess.PIPE,
        text=True,
    )
    follow_id = None
    saw_waiting = False
    saw_plan_created = False
    deadline = time.time() + 20
    while time.time() < deadline:
        line = follow.stdout.readline()
        if not line:
            break
        obj = json.loads(line)
        if obj.get("type") == "event":
            follow_id = follow_id or obj.get("run_id")
            if obj.get("event_type") == "run.waiting_approval":
                saw_waiting = True
                break
            if obj.get("event_type") == "plan.created":
                saw_plan_created = True
    if follow_id:
        jrun("run", "approve", follow_id, "--scope", "once")
        terminal = None
        deadline = time.time() + 20
        while time.time() < deadline:
            line = follow.stdout.readline()
            if not line:
                break
            obj = json.loads(line)
            if obj.get("type") == "receipt":
                terminal = obj
                break
        follow.wait(timeout=10)
        RESULT["jsonl"] = (
            saw_waiting
            and saw_plan_created
            and terminal is not None
            and terminal["terminal_state"] == "completed"
            and follow.returncode == 0
        )
    else:
        follow.kill()

    # 7) MCP: consultar el plan y aprobar.
    mcp = Mcp(ENV)
    try:
        mcp.call("initialize", {"protocolVersion": "2024-11-05", "capabilities": {}})
        listed = mcp.call("tools/list")
        names = {t["name"] for t in listed["result"]["tools"]}
        mcp_plan = jrun(
            "run", "start", "--workspace", ws, "--thread", th, "--branch", br,
            "--prompt", "tool:write mcp.txt m", "--provider", "sim",
        )
        got = mcp.call("tools/call", {"name": "run_plan", "arguments": {"run_id": mcp_plan["id"]}})
        plan_payload = json.loads(got["result"]["content"][0]["text"])
        approved_mcp = mcp.call(
            "tools/call",
            {"name": "run_approve",
             "arguments": {"run_id": mcp_plan["id"],
                           "plan_id": plan_payload["plan"]["plan_id"],
                           "scope": "once"}},
        )
        finished = wait_state(ENV, mcp_plan["id"], "completed")
        RESULT["mcp"] = (
            {"run_plan", "run_approve", "run_reject", "run_replan"} <= names
            and got["result"]["isError"] is False
            and approved_mcp["result"]["isError"] is False
            and finished["state"] == "completed"
        )
    finally:
        mcp.close()

    # 8) Export/import: planes, revisiones y procedencia.
    archive = HOME / "plan-workspace.hliar"
    jrun("export", ws, "--out", str(archive))
    home2 = Path(tempfile.mkdtemp(prefix="hli-plan-import-"))
    env2 = {**os.environ, "HLI_HOME": str(home2)}
    daemon2 = start_daemon(env2)
    try:
        jrun("import", str(archive), env=env2)
        imported = jrun("run", "plan", write_run["id"], env=env2)
        RESULT["export_import"] = (
            imported["plan"]["plan_id"] == shown["plan"]["plan_id"]
            and imported["plan"]["approved_by"] is not None
            and imported["plan"]["content_hash"] == shown["plan"]["content_hash"]
            and len(imported["plan"]["steps"]) == len(shown["plan"]["steps"])
        )
    finally:
        stop_daemon(env2, daemon2)
        shutil.rmtree(home2, ignore_errors=True)

    RESULT["ok"] = all(
        RESULT[k]
        for k in RESULT
        if k not in ("gate", "ok")
    )
finally:
    try:
        stop_daemon(ENV, daemon)
    except Exception:
        pass
    shutil.rmtree(HOME, ignore_errors=True)
    shutil.rmtree(PROJECT, ignore_errors=True)

print(json.dumps(RESULT, indent=2))
sys.exit(0 if RESULT["ok"] else 1)
