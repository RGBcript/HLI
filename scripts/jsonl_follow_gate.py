#!/usr/bin/env python3
"""Gate de `hli run --output jsonl --follow` (HLI 0.9)."""
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

result = {"gate": "jsonl-follow", "ok": False, "ordered": False, "no_duplicates": False,
          "no_ansi": False, "receipt_projection": False, "cancel": False,
          "concurrent_streams": False,
          "consumer_disconnect_safe": False, "backward_compatible": False}

home = Path(tempfile.mkdtemp(prefix="hli-follow-gate-"))
project = Path(tempfile.mkdtemp(prefix="hli-follow-project-"))
env = {**os.environ, "HLI_HOME": str(home)}


def run(args, cwd=None, timeout=120):
    return subprocess.run([str(HLI)] + args, capture_output=True, text=True, env=env,
                          cwd=cwd, timeout=timeout)


def jrun(args, cwd=None):
    return json.loads(run(args + ["--output", "json"], cwd).stdout)


try:
    run(["daemon", "start"])
    ws = jrun(["workspace", "create", "follow", "--root", str(project)], project)["id"]
    th = jrun(["thread", "create", "follow", "--workspace", ws], project)["id"]
    br = jrun(["branch", "create", "main", "--thread", th], project)["id"]

    # follow completo
    p = run(["run", "tarea follow", "--async", "--output", "jsonl", "--follow",
             "--sim-delay-ms", "800", "--timeout-secs", "8"], project)
    lines = [l for l in p.stdout.splitlines() if l.strip()]
    parsed = [json.loads(l) for l in lines]
    events = [x for x in parsed if x.get("type") == "event"]
    receipts = [x for x in parsed if x.get("type") == "receipt"]
    seqs = [e["sequence"] for e in events]
    result["ordered"] = seqs == sorted(seqs) and len(seqs) > 0
    result["no_duplicates"] = len(seqs) == len(set(seqs))
    result["no_ansi"] = "\x1b" not in p.stdout
    result["receipt_projection"] = (
        len(receipts) == 1
        and receipts[0]["source"] == "run.status"
        and receipts[0]["terminal_state"] == "completed"
        and "receipt" in receipts[0]
    )
    completed_exit = p.returncode == 0 and receipts[0]["terminal_state"] == "completed"

    # follow con timeout provoca timed_out y exit != 0
    p_timeout = run(["run", "tarea lenta", "--async", "--output", "jsonl", "--follow",
                     "--sim-delay-ms", "2500", "--timeout-secs", "1"], project)
    failed_ok = p_timeout.returncode != 0 and "\"terminal_state\":\"timed_out\"" in p_timeout.stdout.replace(" ", "")

    # cancelación desde otro cliente mientras el consumidor sigue
    cancel_started = time.time()
    proc = subprocess.Popen([str(HLI), "run", "tarea cancelable", "--async", "--output", "jsonl",
                             "--follow", "--sim-delay-ms", "6000", "--timeout-secs", "30",
                             "--cancel-timeout-secs", "1"],
                            stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True,
                            env=env, cwd=project)
    run_id = None
    deadline = time.time() + 15
    while time.time() < deadline and run_id is None:
        line = proc.stdout.readline()
        if not line:
            break
        obj = json.loads(line)
        if obj.get("type") == "event" and obj.get("run_id"):
            run_id = obj["run_id"]
    if run_id:
        run(["run", "cancel", run_id], project)
    rest = proc.stdout.read()
    proc.wait(timeout=30)
    cancel_lines = [json.loads(l) for l in rest.splitlines() if l.strip()]
    cancel_receipts = [x for x in cancel_lines if x.get("type") == "receipt"]
    result["cancel"] = (
        bool(cancel_receipts)
        and cancel_receipts[-1]["terminal_state"] == "cancelled"
        and proc.returncode != 0
        and (time.time() - cancel_started) < 10
    )

    # dos runs concurrentes: cada stream contiene solo sus eventos
    def start_follow(task: str, delay: int):
        return subprocess.Popen([str(HLI), "run", task, "--async", "--output", "jsonl",
                                 "--follow", "--sim-delay-ms", str(delay),
                                 "--timeout-secs", "10"],
                                stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, text=True,
                                env=env, cwd=project)

    pa = start_follow("concurrente A", 1500)
    pb = start_follow("concurrente B", 300)
    out_a, out_b = pa.communicate(timeout=40)[0], pb.communicate(timeout=40)[0]

    def own_only(text: str) -> bool:
        lines = [json.loads(l) for l in text.splitlines() if l.strip()]
        evs = [x for x in lines if x.get("type") == "event"]
        if not evs:
            return False
        rid = evs[0]["run_id"]
        return all((e.get("data") or {}).get("run_id") == rid for e in evs) and all(
            e["run_id"] == rid for e in evs
        )

    result["concurrent_streams"] = own_only(out_a) and own_only(out_b)

    # consumidor que cierra la tubería: el daemon sigue y el run termina
    p2 = subprocess.Popen([str(HLI), "run", "tarea pipe", "--async", "--output", "jsonl",
                           "--follow", "--sim-delay-ms", "600", "--timeout-secs", "8"],
                          stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, text=True,
                          env=env, cwd=project)
    p2.stdout.readline()
    p2.stdout.close()
    p2.wait(timeout=30)
    time.sleep(1.0)
    status = jrun(["daemon", "status"])
    runs = jrun(["run", "list", "--workspace", ws])
    last = runs[-1]["state"] if runs else ""
    result["consumer_disconnect_safe"] = status.get("running") is True and last != "cancelled"

    # jsonl sin follow: una sola línea
    p3 = run(["run", "tarea simple", "--output", "jsonl"], project)
    single = [l for l in p3.stdout.splitlines() if l.strip()]
    obj = json.loads(single[0]) if len(single) == 1 else {}
    result["backward_compatible"] = len(single) == 1 and "id" in obj and obj.get("type") is None

    result["ok"] = all([
        result["ordered"], result["no_duplicates"], result["no_ansi"],
        result["receipt_projection"], completed_exit, failed_ok, result["cancel"],
        result["concurrent_streams"],
        result["consumer_disconnect_safe"], result["backward_compatible"],
    ])
except Exception as exc:  # noqa: BLE001
    result["error"] = str(exc)
finally:
    try:
        run(["daemon", "stop"])
    except Exception:
        pass
    shutil.rmtree(home, ignore_errors=True)
    shutil.rmtree(project, ignore_errors=True)

print(json.dumps(result, indent=2))
sys.exit(0 if result["ok"] else 1)
