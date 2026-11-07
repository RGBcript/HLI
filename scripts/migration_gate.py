#!/usr/bin/env python3
"""Gate de migraciones: fixture previa real, backup, apply, verificación, MCH,
memoria, export/import y rollback ante una migración fallida."""
import hashlib
import json
import os
import shutil
import sqlite3
import subprocess
import sys
import tempfile
import time

ROOT = r"C:\Users\GUERRA\Documents\TEC\HLI"
DEBUG = os.path.join(ROOT, "target", "debug")
HLI = os.path.join(DEBUG, "hli.exe")
DAEMON = os.path.join(DEBUG, "hli-daemon.exe")
FIXTURE = os.path.join(DEBUG, "hli-module-fixture.exe")
WS = "ws_migration_fixture"
TH = "th_migration_fixture"
BR = "br_migration_fixture"
MCH = "migration-main"
ARTIFACT_BODY = b"contenido de artifact previo"


def sha(data):
    return "sha256:" + hashlib.sha256(data).hexdigest()


def run(env, *args, check=True, timeout=120):
    p = subprocess.run([HLI, *args], env=env, capture_output=True, text=True, timeout=timeout)
    if check and p.returncode != 0:
        raise RuntimeError(f"{args}: {p.stderr}{p.stdout}")
    return p


def jrun(env, *args):
    return json.loads(run(env, *args).stdout)


def make_fixture(home):
    shutil.rmtree(home, ignore_errors=True)
    subprocess.run(
        ["cargo", "run", "-q", "-p", "hli-store", "--example",
         "make_migration_fixture", "--", home],
        cwd=ROOT, check=True, capture_output=True, text=True,
    )
    module = os.path.join(home, "modules", "migration-module", "0.1.0", "bin")
    os.makedirs(module, exist_ok=True)
    shutil.copy(FIXTURE, os.path.join(module, "hli-module-fixture.exe"))


def start_daemon(env):
    d = subprocess.Popen(
        [DAEMON], env=env, stdin=subprocess.DEVNULL,
        stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
    )
    for _ in range(120):
        time.sleep(0.05)
        r = run(env, "daemon", "status", check=False)
        try:
            if json.loads(r.stdout)["running"]:
                return d
        except Exception:
            pass
    raise RuntimeError("daemon no arrancó")


def stop_daemon(env, d):
    run(env, "daemon", "stop", check=False)
    try:
        d.wait(timeout=5)
    except Exception:
        d.kill()


def main():
    base = tempfile.mkdtemp(prefix="hli-migration-gate-")
    home1 = os.path.join(base, "primary")
    home2 = os.path.join(base, "imported")
    home3 = os.path.join(base, "rollback")
    env1 = dict(os.environ, HLI_HOME=home1)
    env2 = dict(os.environ, HLI_HOME=home2)
    env3 = dict(os.environ, HLI_HOME=home3)
    out = {"gate": "migration", "ok": False}
    d1 = d2 = None
    phase = "fixture"
    try:
        make_fixture(home1)
        status = jrun(env1, "migrate", "status")
        out["previous_fixture"] = (
            status["current_version"] == 5 and status["target_version"] == 7
        )
        if not out["previous_fixture"]:
            raise RuntimeError(f"fixture inesperada: {status}")

        before = sqlite3.connect(os.path.join(home1, "hli.db"))
        head_before = before.execute(
            "SELECT event_hash FROM events WHERE workspace_id=? ORDER BY sequence DESC LIMIT 1",
            (WS,),
        ).fetchone()[0]
        before.close()

        phase = "backup"
        manual = jrun(env1, "migrate", "backup")
        out["backup_created"] = os.path.exists(manual["backup"])

        phase = "apply"
        applied = jrun(env1, "migrate", "apply")
        out["schema_upgraded"] = applied["to_version"] == 7 and applied["from_version"] == 5
        if applied.get("backup") is None:
            out["backup_created"] = False

        phase = "verify"
        verify = jrun(env1, "migrate", "verify")
        after = sqlite3.connect(os.path.join(home1, "hli.db"))
        head_after = after.execute(
            "SELECT event_hash FROM events WHERE workspace_id=? ORDER BY sequence DESC LIMIT 1",
            (WS,),
        ).fetchone()[0]
        after.close()
        out["events_preserved"] = (
            verify["chain_ok"] and verify["schema_ok"] and verify["events_checked"] >= 3
            and head_before == head_after
        )

        phase = "daemon"
        d1 = start_daemon(env1)
        served = jrun(env1, "mch", "serve", MCH)
        invoked = jrun(env1, "mch", "invoke", MCH, "memory.search",
                       "--arguments", '{"query":""}')
        out["mch_preserved"] = (
            served["pid"] == invoked["result"]["pid"]
            and jrun(env1, "mch", "status", MCH)["instance"]["state"] == "ready"
        )

        phase = "memory"
        memories = jrun(env1, "memory", "search", "--workspace", WS, "--query", "dato")
        text = json.dumps(memories, ensure_ascii=False)
        out["memory_preserved"] = "dato vigente" in text and "dato antiguo" not in text

        phase = "run"
        run(env1, "run", "start", "--workspace", WS, "--thread", TH, "--branch", BR,
            "--prompt", "después de migrar", "--provider", "sim")
        verify_ws = jrun(env1, "verify", "--workspace", WS)
        if not verify_ws.get("ok"):
            raise RuntimeError(f"verify de workspace falló: {verify_ws}")

        phase = "artifacts"
        artifacts = jrun(env1, "artifact", "list", "--workspace", WS)
        expected = sha(ARTIFACT_BODY).removeprefix("sha256:")
        out["artifacts_preserved"] = any(a.get("sha256") == expected for a in artifacts)
        if not out["artifacts_preserved"]:
            raise RuntimeError(f"artifact no preservado: {artifacts}")

        phase = "export"
        archive = os.path.join(base, "workspace.hliar")
        run(env1, "export", WS, "--out", archive)
        stop_daemon(env1, d1)
        d1 = None

        os.makedirs(home2, exist_ok=True)
        d2 = start_daemon(env2)
        jrun(env2, "import", archive)
        imported_verify = jrun(env2, "verify", "--workspace", WS)
        imported_mem = json.dumps(
            jrun(env2, "memory", "search", "--workspace", WS, "--query", "dato"),
            ensure_ascii=False,
        )
        imported_art = jrun(env2, "artifact", "list", "--workspace", WS)
        out["export_import_verified"] = (
            imported_verify.get("ok")
            and "dato vigente" in imported_mem
            and any(a.get("sha256") == expected for a in imported_art)
        )
        stop_daemon(env2, d2)
        d2 = None

        phase = "rollback"
        make_fixture(home3)
        env_fail = dict(env3, HLI_MIGRATION_FAIL_AT="6")
        failed = run(env_fail, "migrate", "apply", check=False)
        rolled = jrun(env3, "migrate", "status")
        out["rollback_verified"] = (
            failed.returncode != 0
            and rolled["current_version"] == 5
            and "migration_failed" in (failed.stderr + failed.stdout)
        )
        if not out["rollback_verified"]:
            raise RuntimeError(f"rollback inesperado: {failed.stderr}{failed.stdout} {rolled}")

        out["ok"] = all([
            out["previous_fixture"],
            out["backup_created"],
            out["schema_upgraded"],
            out["events_preserved"],
            out["artifacts_preserved"],
            out["memory_preserved"],
            out["mch_preserved"],
            out["rollback_verified"],
            out["export_import_verified"],
        ])
        print(json.dumps(out, indent=2))
    except Exception as exc:
        print(json.dumps({**out, "ok": False, "phase": phase, "error": str(exc)}, indent=2))
        return 1
    finally:
        if d1 is not None:
            stop_daemon(env1, d1)
        if d2 is not None:
            stop_daemon(env2, d2)
        shutil.rmtree(base, ignore_errors=True)
    return 0 if out["ok"] else 1


if __name__ == "__main__":
    sys.exit(main())
