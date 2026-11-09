#!/usr/bin/env python3
"""Gate consolidado de Agent Mode (HLI 0.9).

Un solo entorno limpio recorre: linking, thread implícito, plan, aprobación,
permisos por invocación, revisión por tool no declarada, referencias explícitas,
actividad/transcript, resume, Inspector, async/cancelación, receipt, artefactos,
JSONL, MCP, MCH y seguridad de secretos.

`manual_pending` permanece true: la validación humana no se automatiza.
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
FIXTURE = ROOT / "target" / "fixture" / "debug" / (
    "hli-daemon.exe" if os.name == "nt" else "hli-daemon"
)

RESULT = {
    "gate": "agent-tui",
    "ok": False,
    "workspace_linking": False,
    "durable_thread": False,
    "planning": False,
    "plan_approval": False,
    "tool_permissions": False,
    "plan_revision": False,
    "explicit_references": False,
    "activity": False,
    "transcript": False,
    "resume": False,
    "inspector": False,
    "async_run": False,
    "cancellation": False,
    "receipt": False,
    "artifacts": False,
    "jsonl_follow": False,
    "mcp": False,
    "mch": False,
    "secret_safety": False,
    "manual_pending": True,
}

HOME = Path(tempfile.mkdtemp(prefix="hli-tui-gate-"))
PROJECT = Path(tempfile.mkdtemp(prefix="hli-tui-project-"))
ENV = {**os.environ, "HLI_HOME": str(HOME)}
SECRET = "TUIGATE-SECRET-42"


def run(*args, cwd=PROJECT, env=None, timeout=180):
    return subprocess.run([str(HLI), *args], capture_output=True, text=True,
                          env=env or ENV, cwd=str(cwd), timeout=timeout)


def jrun(*args, cwd=PROJECT, env=None):
    p = run(*args, "--output", "json", cwd=cwd, env=env)
    if p.returncode != 0:
        raise RuntimeError(f"{args}: {p.stderr or p.stdout}")
    return json.loads(p.stdout)


def start_daemon(env, binary=None):
    proc = subprocess.Popen([str(binary or DAEMON)], env=env,
                            stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL,
                            stderr=subprocess.DEVNULL)
    for _ in range(200):
        time.sleep(0.05)
        p = subprocess.run([str(HLI), "daemon", "status"], env=env,
                           capture_output=True, text=True)
        if p.returncode == 0 and '"running":true' in p.stdout.replace(" ", ""):
            return proc
    raise RuntimeError("daemon no arrancó")


def stop_daemon(env, proc):
    if proc is None:
        return
    subprocess.run([str(HLI), "daemon", "stop"], env=env, capture_output=True)
    try:
        proc.wait(timeout=5)
    except Exception:
        proc.kill()


def wait_state(run_id, states, env=None, timeout=20):
    deadline = time.time() + timeout
    last = None
    while time.time() < deadline:
        last = jrun("run", "status", run_id, env=env)["run"]
        if last["state"] in states:
            return last
        time.sleep(0.1)
    return last


def approve_pending(run_id, scope="once", env=None):
    pending = jrun("permission", "list", "--run", run_id, "--status", "pending", env=env)
    for req in pending:
        jrun("permission", "approve", req["request_id"], "--scope", scope, env=env)


daemon = None
fdaemon = None
try:
    daemon = start_daemon(ENV)

    # 1) Linking y thread implícito. Se crea el workspace como hace Agent Mode
    # al confirmar el directorio; el thread y la rama los crea el propio run.
    created = jrun("workspace", "create", "agent-gate", "--root", str(PROJECT))
    run("run", "smoke de agent mode", "--output", "json")
    spaces = jrun("workspace", "list")
    RESULT["workspace_linking"] = len(spaces) == 1 and Path(
        spaces[0]["root_path"]
    ).name == PROJECT.name
    ws = created["id"]
    early = jrun("run", "list", "--workspace", ws)
    RESULT["durable_thread"] = bool(early) and all(
        r["thread_id"] and r["branch_id"] for r in early
    )
    th = early[0]["thread_id"]
    br = early[0]["branch_id"]
    branches = jrun("branch", "list", "--thread", th)
    RESULT["durable_thread"] = RESULT["durable_thread"] and any(
        b["name"] == "main" for b in branches
    )

    # 2) Plan, aprobación y permiso por invocación.
    target = PROJECT / "gate.txt"
    wrun = jrun("run", "start", "--workspace", ws, "--thread", th, "--branch", br,
                "--prompt", "tool:write gate.txt ok")
    RESULT["planning"] = wrun["state"] == "waiting_approval"
    plan = jrun("run", "plan", wrun["id"])
    jrun("run", "approve", wrun["id"], "--scope", "once", "--plan", plan["plan"]["plan_id"])
    waiting = wait_state(wrun["id"], ("waiting_approval", "completed", "failed"))
    RESULT["plan_approval"] = waiting["state"] == "waiting_approval" and not target.exists()
    approve_pending(wrun["id"], "once")
    done = wait_state(wrun["id"], ("completed", "failed", "cancelled"))
    RESULT["tool_permissions"] = bool(
        done["state"] == "completed"
        and target.exists()
        and done["receipt"].get("permission_decisions")
    )
    RESULT["receipt"] = bool(
        done["receipt"].get("terminal_event")
        and done["receipt"].get("plan_id")
        and done["receipt"].get("plan_revision")
    )

    # 3) Revisión por tool no declarada con el daemon fixture.
    build = subprocess.run(
        ["cargo", "build", "-p", "hli-daemon", "--features", "fixture-provider",
         "--target-dir", "target/fixture"],
        cwd=str(ROOT), capture_output=True, text=True,
    )
    if build.returncode == 0 and FIXTURE.exists():
        stop_daemon(ENV, daemon)
        fhome = Path(tempfile.mkdtemp(prefix="hli-tui-fixture-"))
        fproj = Path(tempfile.mkdtemp(prefix="hli-tui-fixture-proj-"))
        fenv = {**os.environ, "HLI_HOME": str(fhome)}
        fdaemon = start_daemon(fenv, FIXTURE)
        fws = jrun("workspace", "create", "rev", "--root", str(fproj), env=fenv)["id"]
        fth = jrun("thread", "create", "rev", "--workspace", fws, env=fenv)["id"]
        fbr = jrun("branch", "create", "main", "--thread", fth, env=fenv)["id"]
        rev = jrun("run", "start", "--workspace", fws, "--thread", fth,
                   "--branch", fbr, "--prompt", "fixture", "--provider", "fixture",
                   env=fenv)["id"]
        st = wait_state(rev, ("waiting_approval",), env=fenv)
        p2 = jrun("run", "plan", rev, env=fenv)
        rev_file = fproj / "revision-test.txt"
        ok_before = st["approval_kind"] == "plan" and p2["plan"]["revision"] == 2
        jrun("run", "approve", rev, "--scope", "once",
             "--plan", p2["plan"]["plan_id"], env=fenv)
        wait_state(rev, ("waiting_approval",), env=fenv)
        ok_mid = not rev_file.exists()
        approve_pending(rev, "once", env=fenv)
        done_rev = wait_state(rev, ("completed", "failed"), env=fenv)
        stop_daemon(fenv, fdaemon)
        fdaemon = start_daemon(fenv, FIXTURE)
        after = jrun("run", "plan", rev, env=fenv)
        RESULT["plan_revision"] = bool(
            ok_before
            and ok_mid
            and done_rev["state"] == "completed"
            and rev_file.read_text(encoding="utf-8") == "written-after-revision"
            and len(after["revisions"]) == 2
            and after["plan"]["revision"] == 2
        )
        stop_daemon(fenv, fdaemon)
        fdaemon = None
        shutil.rmtree(fhome, ignore_errors=True)
        shutil.rmtree(fproj, ignore_errors=True)
        daemon = start_daemon(ENV)
    else:
        daemon = start_daemon(ENV)

    # 4) Referencias explícitas.
    (PROJECT / "nota.txt").write_text("contenido de nota", encoding="utf-8")
    refs_arg = json.dumps([{
        "reference_type": "file", "stable_id": "nota.txt",
        "display_text": "nota.txt", "selection_reason": "user_selected",
    }])
    rrun = jrun("run", "start", "--workspace", ws, "--thread", th, "--branch", br,
                "--prompt", "usa la referencia", "--explicit-references", refs_arg)
    got = jrun("run", "get", rrun["id"])
    refs = got["manifest"]["explicit_references"]
    RESULT["explicit_references"] = bool(refs) and refs[0]["validation_state"] == "valid"

    # 5) Actividad, transcript y resume (mismos datos que reconstruye la TUI).
    events = jrun("event", "list", "--workspace", ws)
    seqs = [e["sequence"] for e in events]
    kinds = {e["type"] for e in events}
    RESULT["activity"] = seqs == sorted(seqs) and bool(
        kinds & {"tool.completed", "plan.approved", "model.requested"}
    )
    RESULT["transcript"] = bool(
        got["run"]["receipt"]
        and isinstance(got["manifest"]["visible_event_ids"], list)
        and seqs == sorted(seqs)
    )
    RESULT["resume"] = bool(
        jrun("thread", "list", "--workspace", ws)
        and jrun("run", "list", "--workspace", ws)
    )

    # 6) Inspector: las fuentes que consume la TUI.
    inspector_ok = True
    for args in [
        ("run", "get", rrun["id"]),
        ("run", "plan", rrun["id"]),
        ("memory", "list", "--workspace", ws),
        ("artifact", "list", "--workspace", ws),
        ("permission", "list"),
    ]:
        inspector_ok = inspector_ok and run(*args, "--output", "json").returncode == 0
    RESULT["inspector"] = inspector_ok

    # 7) Async + cancelación.
    arun = jrun("run", "start", "--workspace", ws, "--thread", th, "--branch", br,
                "--prompt", "tarea lenta", "--async", "--sim-delay-ms", "1500")
    RESULT["async_run"] = arun["state"] in ("queued", "running", "completed")
    jrun("run", "cancel", arun["id"])
    cancelled = wait_state(arun["id"], ("cancelled", "failed", "completed"))
    RESULT["cancellation"] = cancelled["state"] == "cancelled"

    # 8) Artefactos.
    art = jrun("artifact", "put", str(PROJECT / "nota.txt"), "--workspace", ws)
    arts = jrun("artifact", "list", "--workspace", ws)
    RESULT["artifacts"] = bool(art.get("id")) and any(a["id"] == art["id"] for a in arts)

    # 9) JSONL --follow.
    follow = run("run", "tarea follow", "--async", "--output", "jsonl", "--follow",
                 "--sim-delay-ms", "200", "--timeout-secs", "10")
    lines = [json.loads(l) for l in follow.stdout.splitlines() if l.strip()]
    receipts = [l for l in lines if l.get("type") == "receipt"]
    RESULT["jsonl_follow"] = bool(
        receipts and receipts[-1]["terminal_state"] == "completed"
        and follow.returncode == 0
    )

    # 10) MCP: tools/list por stdio.
    proc = subprocess.Popen([str(MCP)], env=ENV, stdin=subprocess.PIPE,
                            stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    try:
        msg = json.dumps({"jsonrpc": "2.0", "id": 1, "method": "tools/list",
                          "params": {}}).encode()
        proc.stdin.write(f"Content-Length: {len(msg)}\r\n\r\n".encode() + msg)
        proc.stdin.flush()
        headers = {}
        while True:
            line = proc.stdout.readline().decode().strip()
            if not line:
                break
            k, _, v = line.partition(":")
            headers[k.strip().lower()] = v.strip()
        body = json.loads(proc.stdout.read(int(headers["content-length"])))
        names = {t["name"] for t in body["result"]["tools"]}
        RESULT["mcp"] = {"permission_approve", "run_plan", "mch_invoke"} <= names
    finally:
        proc.kill()

    # 11) MCH: registro operativo y módulo de memoria empaquetado.
    RESULT["mch"] = bool(
        run("mch", "list", "--output", "json").returncode == 0
        and list((ROOT / "dist").glob("hli-memory-*.hlimod"))
    )

    # 12) Secret safety: ni DB ni export contienen el valor sintético.
    (HOME / "secrets").mkdir(exist_ok=True)
    (HOME / "secrets" / "tuigate").write_text(SECRET, encoding="utf-8")
    jrun("provider", "add", "tuigate-oai", "--kind", "openai",
         "--base-url", "http://127.0.0.1:9/v1", "--model", "m", "--timeout", "2",
         "--secret-handle", "tuigate")
    srun = jrun("run", "start", "--workspace", ws, "--thread", th, "--branch", br,
                "--prompt", "tool:write secret-tui.txt s", "--provider", "tuigate-oai")
    plan_s = jrun("run", "plan", srun["id"])
    run("run", "approve", srun["id"], "--scope", "once",
        "--plan", plan_s["plan"]["plan_id"], "--output", "json")
    wait_state(srun["id"], ("failed", "waiting_approval", "completed"))
    archive = HOME / "tui-gate.hliar"
    jrun("export", ws, "--out", str(archive))
    blobs = [(HOME / "hli.db").read_bytes(), archive.read_bytes(),
             json.dumps(jrun("event", "list", "--workspace", ws)).encode()]
    RESULT["secret_safety"] = all(SECRET.encode() not in b for b in blobs)

    RESULT["ok"] = all(v for k, v in RESULT.items()
                       if k not in ("gate", "ok", "manual_pending"))
finally:
    stop_daemon(ENV, daemon)
    stop_daemon(ENV, fdaemon)
    shutil.rmtree(HOME, ignore_errors=True)
    shutil.rmtree(PROJECT, ignore_errors=True)

print(json.dumps(RESULT, indent=2))
sys.exit(0 if RESULT["ok"] else 1)
