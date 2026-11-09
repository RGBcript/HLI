#!/usr/bin/env python3
"""Gate de referencias explícitas (HLI 0.9).

Demuestra con daemon y SimProvider reales:
file + memory en ContextManifest, payload_hash, archivo modificado, refresh,
memoria superseded rechazada y export/import.

Se ejecuta en Python porque el orquestador PowerShell se bloqueaba al
redirigir los flujos del daemon; los comandos son exactamente los mismos.
"""
from __future__ import annotations

import hashlib
import json
import os
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
HLI = ROOT / "target" / "debug" / "hli.exe"

result = {
    "gate": "explicit-references",
    "ok": False,
    "file": False,
    "memory": False,
    "payload_hash": False,
    "changed_detected": False,
    "refresh": False,
    "invalid_rejected": False,
    "export_import": False,
}


def run(home: Path, args: list[str], cwd: Path | None = None, expect_fail: bool = False):
    env = {**os.environ, "HLI_HOME": str(home)}
    p = subprocess.run(
        [str(HLI)] + args, capture_output=True, text=True, env=env, cwd=cwd, timeout=120
    )
    if not expect_fail and p.returncode != 0:
        raise RuntimeError(p.stderr.strip() or p.stdout.strip() or f"falló {args}")
    return p


def jrun(home: Path, args: list[str], cwd: Path | None = None):
    return json.loads(run(home, args + ["--output", "json"], cwd).stdout)


def sha(path: Path) -> str:
    return "sha256:" + hashlib.sha256(path.read_bytes()).hexdigest()


def write_refs(path: Path, refs: list[dict]) -> Path:
    path.write_text(json.dumps(refs), encoding="utf-8")
    return path


home = Path(tempfile.mkdtemp(prefix="hli-ref-gate-"))
project = Path(tempfile.mkdtemp(prefix="hli-ref-project-"))
home2 = None
try:
    (project / "a.txt").write_text("contenido inicial\n", encoding="utf-8")
    run(home, ["daemon", "start"])
    ws = jrun(home, ["workspace", "create", "refs", "--root", str(project)], project)["id"]
    th = jrun(home, ["thread", "create", "refs", "--workspace", ws], project)["id"]
    br = jrun(home, ["branch", "create", "main", "--thread", th], project)["id"]
    mem = jrun(
        home,
        ["memory", "commit", "--workspace", ws, "--type", "fact",
         "--content", "SQLite es la fuente", "--scope", "workspace"],
        project,
    )["id"]
    old = jrun(
        home,
        ["memory", "commit", "--workspace", ws, "--type", "fact",
         "--content", "PostgreSQL era la fuente", "--scope", "workspace"],
        project,
    )["id"]
    run(
        home,
        ["memory", "commit", "--workspace", ws, "--type", "fact",
         "--content", "SQLite manda", "--scope", "workspace", "--supersedes", old],
        project,
    )

    refs = write_refs(
        project / "refs.json",
        [
            {"reference_type": "file", "stable_id": "a.txt", "display_text": "a.txt",
             "content_hash": sha(project / "a.txt"), "selection_reason": "user_selected",
             "selected_at": "1700000000"},
            {"reference_type": "memory", "stable_id": mem, "display_text": "SQLite",
             "selection_reason": "user_selected", "selected_at": "1700000001"},
        ],
    )
    run_args = ["run", "start", "--workspace", ws, "--thread", th, "--branch", br,
                "--prompt", "usa referencias", "--provider", "sim",
                "--explicit-references-file", str(refs)]
    run1 = jrun(home, run_args, project)
    ctx = run1["receipt"]["context_manifest"]
    manifest = jrun(home, ["manifest", "get", ctx], project)
    manifest = manifest.get("manifest", manifest)
    entries = manifest.get("explicit_references", [])
    result["file"] = run1["state"] == "completed" and any(e["reference_type"] == "file" for e in entries)
    result["memory"] = run1["state"] == "completed" and any(e["reference_type"] == "memory" for e in entries)
    result["payload_hash"] = bool(manifest.get("payload_hash"))

    (project / "a.txt").write_text("contenido cambiado\n", encoding="utf-8")
    p = run(home, run_args, project, expect_fail=True)
    result["changed_detected"] = p.returncode != 0 and "reference_changed" in (p.stderr + p.stdout)

    refs2 = write_refs(project / "refs2.json",
                       [{"reference_type": "file", "stable_id": "a.txt",
                         "content_hash": sha(project / "a.txt"),
                         "selection_reason": "user_selected", "selected_at": "1700000002"}])
    run2 = jrun(home, ["run", "start", "--workspace", ws, "--thread", th, "--branch", br,
                       "--prompt", "referencia refrescada", "--provider", "sim",
                       "--explicit-references-file", str(refs2)], project)
    result["refresh"] = run2["state"] == "completed"

    refs3 = write_refs(project / "refs3.json",
                       [{"reference_type": "memory", "stable_id": old,
                         "selection_reason": "user_selected", "selected_at": "1700000003"}])
    p3 = run(home, ["run", "start", "--workspace", ws, "--thread", th, "--branch", br,
                    "--prompt", "memoria vieja", "--provider", "sim",
                    "--explicit-references-file", str(refs3)], project, expect_fail=True)
    result["invalid_rejected"] = p3.returncode != 0 and "reference_" in (p3.stderr + p3.stdout)

    archive = project / "ws.hliar"
    run(home, ["export", ws, "--out", str(archive)], project)
    home2 = Path(tempfile.mkdtemp(prefix="hli-ref-import-"))
    run(home2, ["daemon", "start"])
    run(home2, ["import", str(archive)], project)
    imported = jrun(home2, ["manifest", "get", ctx], project)
    imported = imported.get("manifest", imported)
    result["export_import"] = len(imported.get("explicit_references", [])) >= 1

    result["ok"] = all(
        result[k]
        for k in ("file", "memory", "payload_hash", "changed_detected",
                  "refresh", "invalid_rejected", "export_import")
    )
except Exception as exc:  # noqa: BLE001
    result["error"] = str(exc)
finally:
    for h in (home, home2):
        if h:
            try:
                run(h, ["daemon", "stop"], expect_fail=True)
            except Exception:
                pass
    shutil.rmtree(home, ignore_errors=True)
    shutil.rmtree(project, ignore_errors=True)
    if home2:
        shutil.rmtree(home2, ignore_errors=True)

print(json.dumps(result, indent=2))
sys.exit(0 if result["ok"] else 1)
