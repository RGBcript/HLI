#!/usr/bin/env python3
"""Gate de permisos por invocación (HLI 0.10).

Demuestra el flujo completo: plan aprobado → solicitud de permiso durable →
decisión → ejecución exactamente una vez, con scopes once/run/workspace,
rechazo, reinicio, JSONL, MCP, export/import y ausencia de secretos.
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
    "gate": "tool-permissions",
    "ok": False,
    "request_durable": False,
    "no_execution_before_permission": False,
    "allow_once": False,
    "once_consumed": False,
    "allow_run": False,
    "run_scope_expired": False,
    "allow_workspace": False,
    "workspace_persisted": False,
    "deny": False,
    "restart": False,
    "plan_revision": False,
    "receipt": False,
    "jsonl": False,
    "mcp": False,
    "export_import": False,
    "secret_safety": False,
    "tui_state": False,
    "inspector_actions": False,
}

HOME = Path(tempfile.mkdtemp(prefix="hli-perm-gate-"))
PROJECT = Path(tempfile.mkdtemp(prefix="hli-perm-project-"))
ENV = {**os.environ, "HLI_HOME": str(HOME)}


def run(*args, cwd=PROJECT, timeout=120, env=None):
    return subprocess.run(
        [str(HLI), *args], capture_output=True, text=True, env=env or ENV,
        cwd=str(cwd), timeout=timeout,
    )


def jrun(*args, cwd=PROJECT, env=None):
    p = run(*args, "--output", "json", cwd=cwd, env=env)
    if p.returncode != 0:
        raise RuntimeError(f"{args}: {p.stderr or p.stdout}")
    return json.loads(p.stdout)


def start_daemon(env):
    proc = subprocess.Popen(
        [str(DAEMON)], env=env, stdin=subprocess.DEVNULL,
        stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
    )
    for _ in range(200):
        time.sleep(0.05)
        p = subprocess.run([str(HLI), "daemon", "status"], env=env,
                           capture_output=True, text=True)
        if p.returncode == 0 and '"running":true' in p.stdout.replace(" ", ""):
            return proc
    raise RuntimeError("el daemon no arrancó")


def stop_daemon(env, proc):
    subprocess.run([str(HLI), "daemon", "stop"], env=env, capture_output=True)
    try:
        proc.wait(timeout=5)
    except Exception:
        proc.kill()


def wait_state(run_id, states, timeout=10):
    deadline = time.time() + timeout
    last = None
    while time.time() < deadline:
        last = jrun("run", "status", run_id)["run"]
        if last["state"] in states:
            return last
        time.sleep(0.1)
    return last


def open_run(ws, th, br, prompt, root):
    run_id = jrun("run", "start", "--workspace", ws, "--thread", th,
                  "--branch", br, "--prompt", prompt)["id"]
    plan = jrun("run", "plan", run_id)["plan"]
    jrun("run", "approve", run_id, "--scope", "once", "--plan", plan["plan_id"])
    waiting = wait_state(run_id, ("waiting_approval",))
    assert waiting["state"] == "waiting_approval", waiting
    pending = jrun("permission", "list", "--run", run_id, "--status", "pending")
    return run_id, plan, pending


try:
    daemon = start_daemon(ENV)
    ws = jrun("workspace", "create", "perm", "--root", str(PROJECT))["id"]
    th = jrun("thread", "create", "perm", "--workspace", ws)["id"]
    br = jrun("branch", "create", "main", "--thread", th)["id"]

    # 1) allow once
    target = PROJECT / "once.txt"
    run_id, plan, pending = open_run(ws, th, br, "tool:write once.txt hola", PROJECT)
    req = pending[0]
    RESULT["request_durable"] = bool(req["request_id"] and req["arguments_hash"])
    RESULT["no_execution_before_permission"] = not target.exists()
    approved = jrun("permission", "approve", req["request_id"], "--scope", "once")
    done = wait_state(run_id, ("completed", "failed", "cancelled"))
    RESULT["allow_once"] = done["state"] == "completed" and target.read_text() == "hola"
    receipt = done["receipt"]
    RESULT["once_consumed"] = req["request_id"] in (receipt.get("consumed_once_grants") or [])
    RESULT["receipt"] = bool(
        receipt.get("permission_decisions")
        and receipt["permission_decisions"][0]["arguments_hash"] == req["arguments_hash"]
        and receipt["permission_request_ids"] == [req["request_id"]]
        and receipt.get("allowed_tool_invocations") == ["filesystem.write"]
    )
    RESULT["tui_state"] = bool(
        req["tool_name"] and req["target"] and req["effect"] == "write"
    )
    RESULT["inspector_actions"] = bool(approved["request"]["decision"] == "allow")

    # 2) allow run y expiración con el run
    run2, _, pending2 = open_run(ws, th, br, "tool:write run.txt r", PROJECT)
    jrun("permission", "approve", pending2[0]["request_id"], "--scope", "run")
    done2 = wait_state(run2, ("completed", "failed", "cancelled"))
    RESULT["allow_run"] = done2["state"] == "completed"
    run3, _, pending3 = open_run(ws, th, br, "tool:write run3.txt r3", PROJECT)
    RESULT["run_scope_expired"] = bool(pending3) and pending3[0]["status"] == "pending"
    jrun("permission", "deny", pending3[0]["request_id"], "--reason", "gate")

    # 4) deny
    deny_target = PROJECT / "deny.txt"
    run6, _, pending6 = open_run(ws, th, br, "tool:write deny.txt no", PROJECT)
    denied = jrun("permission", "deny", pending6[0]["request_id"], "--reason", "no")
    done6 = wait_state(run6, ("completed", "failed", "cancelled"))
    RESULT["deny"] = bool(
        done6["state"] == "failed"
        and not deny_target.exists()
        and (done6["receipt"].get("denied_tool_invocations") == ["filesystem.write"])
        and denied["request"]["status"] == "denied"
    )

    # 5) reinicio con solicitud pendiente
    run7, _, pending7 = open_run(ws, th, br, "tool:write restart.txt ok", PROJECT)
    stop_daemon(ENV, daemon)
    daemon = start_daemon(ENV)
    recovered = jrun("permission", "list", "--run", run7, "--status", "pending")
    after = jrun("run", "status", run7)["run"]
    jrun("permission", "approve", recovered[0]["request_id"], "--scope", "once")
    done7 = wait_state(run7, ("completed", "failed", "cancelled"))
    RESULT["restart"] = bool(
        after["state"] == "waiting_approval"
        and after["approval_kind"] == "tool"
        and done7["state"] == "completed"
    )

    # 6) secret safety: valor sintético en SQLite, eventos, receipt y export
    secret_value = "GATEPERM-77c1"
    (HOME / "secrets").mkdir(exist_ok=True)
    (HOME / "secrets" / "perm").write_text(secret_value, encoding="utf-8")
    jrun("provider", "add", "perm-oai", "--kind", "openai",
         "--base-url", "http://127.0.0.1:9/v1", "--model", "m",
         "--timeout", "2", "--secret-handle", "perm")
    secret_run = jrun("run", "start", "--workspace", ws, "--thread", th,
                      "--branch", br, "--prompt", "tool:write secret.txt s",
                      "--provider", "perm-oai")["id"]
    plan_s = jrun("run", "plan", secret_run)["plan"]
    jrun("run", "approve", secret_run, "--scope", "once", "--plan", plan_s["plan_id"])
    wait_state(secret_run, ("waiting_approval", "failed", "completed"))
    export = HOME / "perm.hliar"
    jrun("export", ws, "--out", str(export))
    blobs = [
        (HOME / "hli.db").read_bytes(),
        export.read_bytes(),
        json.dumps(jrun("event", "list", "--workspace", ws)).encode(),
        json.dumps(jrun("permission", "list", "--run", run_id)).encode(),
    ]
    RESULT["secret_safety"] = all(secret_value.encode() not in b for b in blobs)

    # 7) JSONL: solicitud, aprobación externa y continuación
    follow = subprocess.Popen(
        [str(HLI), "run", "tool:write jsonl-perm.txt j", "--async",
         "--output", "jsonl", "--follow", "--timeout-secs", "30"],
        env=ENV, cwd=str(PROJECT), stdout=subprocess.PIPE,
        stderr=subprocess.DEVNULL, text=True,
    )
    follow_run = None
    # La primera línea es inmediata (evento del run recién creado); el resto se
    # consume con communicate para no bloquear el gate.
    first = follow.stdout.readline()
    if first.strip():
        follow_run = json.loads(first).get("run_id")
    if follow_run:
        deadline = time.time() + 20
        while time.time() < deadline:
            run_now = jrun("run", "status", follow_run)["run"]
            if run_now["state"] == "waiting_approval":
                if run_now.get("approval_kind") == "plan":
                    shown = jrun("run", "plan", follow_run)["plan"]
                    jrun("run", "approve", follow_run, "--scope", "once",
                         "--plan", shown["plan_id"])
                elif run_now.get("approval_kind") == "tool":
                    break
            time.sleep(0.1)
        pendingj = jrun("permission", "list", "--run", follow_run, "--status", "pending")
        jrun("permission", "approve", pendingj[0]["request_id"], "--scope", "once")
        rest, _ = follow.communicate(timeout=30)
        lines = [json.loads(l) for l in rest.splitlines() if l.strip()]
        events = [l for l in lines if l.get("type") == "event"]
        receipts = [l for l in lines if l.get("type") == "receipt"]
        kinds = [e.get("event_type") for e in events]
        RESULT["jsonl"] = bool(
            "tool.permission_requested" in kinds
            and "tool.permission_approved" in kinds
            and "tool.execution_resumed" in kinds
            and receipts
            and receipts[-1]["terminal_state"] == "completed"
            and follow.returncode == 0
        )
    else:
        follow.kill()

    # 8) MCP: aprobar una solicitud pendiente
    mcp_run, _, pendingm = open_run(ws, th, br, "tool:write mcp.txt m", PROJECT)
    proc = subprocess.Popen([str(MCP)], env=ENV, stdin=subprocess.PIPE,
                            stdout=subprocess.PIPE, stderr=subprocess.PIPE)

    def mcp_call(n, method, params=None):
        msg = json.dumps({"jsonrpc": "2.0", "id": n, "method": method,
                          "params": params or {}}).encode()
        proc.stdin.write(f"Content-Length: {len(msg)}\r\n\r\n".encode() + msg)
        proc.stdin.flush()
        headers = {}
        while True:
            line = proc.stdout.readline().decode().strip()
            if not line:
                break
            k, _, v = line.partition(":")
            headers[k.strip().lower()] = v.strip()
        return json.loads(proc.stdout.read(int(headers["content-length"])))

    mcp_call(1, "initialize", {"protocolVersion": "2024-11-05", "capabilities": {}})
    listed = mcp_call(2, "tools/list")
    names = {t["name"] for t in listed["result"]["tools"]}
    got = mcp_call(3, "tools/call", {"name": "permission_list",
                                    "arguments": {"run_id": mcp_run, "status": "pending"}})
    approved_mcp = mcp_call(4, "tools/call", {
        "name": "permission_approve",
        "arguments": {"request_id": pendingm[0]["request_id"], "scope": "once"}})
    done_mcp = wait_state(mcp_run, ("completed", "failed", "cancelled"))
    proc.kill()
    RESULT["mcp"] = bool(
        {"permission_list", "permission_get", "permission_approve", "permission_deny"} <= names
        and got["result"]["isError"] is False
        and approved_mcp["result"]["isError"] is False
        and done_mcp["state"] == "completed"
    )

    # 3) allow workspace y persistencia
    run4, _, pending4 = open_run(ws, th, br, "tool:write ws1.txt w", PROJECT)
    jrun("permission", "approve", pending4[0]["request_id"], "--scope", "workspace")
    done4 = wait_state(run4, ("completed", "failed", "cancelled"))
    RESULT["allow_workspace"] = done4["state"] == "completed"
    run5 = jrun("run", "start", "--workspace", ws, "--thread", th,
                "--branch", br, "--prompt", "tool:write ws2.txt w2")["id"]
    plan5 = jrun("run", "plan", run5)["plan"]
    jrun("run", "approve", run5, "--scope", "once", "--plan", plan5["plan_id"])
    done5 = wait_state(run5, ("completed", "failed", "cancelled"))
    RESULT["workspace_persisted"] = bool(
        done5["state"] == "completed"
        and not jrun("permission", "list", "--run", run5, "--status", "pending")
    )

    # 9) export/import
    archive = HOME / "permissions.hliar"
    jrun("export", ws, "--out", str(archive))
    home2 = Path(tempfile.mkdtemp(prefix="hli-perm-import-"))
    env2 = {**os.environ, "HLI_HOME": str(home2)}
    daemon2 = start_daemon(env2)
    try:
        jrun("import", str(archive), env=env2)
        imported = jrun("permission", "list", "--run", run_id, env=env2)
        imported_receipt = jrun("run", "get", run_id, env=env2)["run"]["receipt"]
        RESULT["export_import"] = bool(
            imported
            and imported[0]["request_id"] == req["request_id"]
            and imported_receipt.get("permission_request_ids") == [req["request_id"]]
        )
    finally:
        stop_daemon(env2, daemon2)
        shutil.rmtree(home2, ignore_errors=True)

    # plan.revised: sólo es reproducible con una tool no declarada; el runtime
    # sim construye el plan desde el prompt.
    RESULT["plan_revision"] = False
    RESULT["ok"] = all(v for k, v in RESULT.items() if k not in ("gate", "ok"))
finally:
    try:
        stop_daemon(ENV, daemon)
    except Exception:
        pass
    shutil.rmtree(HOME, ignore_errors=True)
    shutil.rmtree(PROJECT, ignore_errors=True)

print(json.dumps(RESULT, indent=2))
sys.exit(0 if RESULT["ok"] else 1)
