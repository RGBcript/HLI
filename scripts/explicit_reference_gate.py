#!/usr/bin/env python3
"""Gate de referencias explícitas (HLI 0.9).

Cubre los cinco tipos, payload_hash, archivo cambiado, Refresh, missing,
superseded, binario seguro, procedencia en receipt, restart y export/import.
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
    "artifact": False,
    "memory": False,
    "run": False,
    "branch": False,
    "payload_hash": False,
    "changed_detected": False,
    "refresh": False,
    "invalid_rejected": False,
    "binary_safe": False,
    "receipt": False,
    "restart": False,
    "export_import": False,
}


def run(home: Path, args: list[str], cwd: Path | None = None, expect_fail: bool = False):
    env = {**os.environ, "HLI_HOME": str(home)}
    p = subprocess.run([str(HLI)] + args, capture_output=True, text=True, env=env, cwd=cwd, timeout=120)
    if not expect_fail and p.returncode != 0:
        raise RuntimeError(f"{args}: {p.stderr.strip() or p.stdout.strip()}")
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
    (project / "bin.dat").write_bytes(bytes([0, 1, 2, 255, 254, 0]))
    (project / "nota.txt").write_text("artefacto de texto\n", encoding="utf-8")
    run(home, ["daemon", "start"])
    ws = jrun(home, ["workspace", "create", "refs", "--root", str(project)], project)["id"]
    th = jrun(home, ["thread", "create", "refs", "--workspace", ws], project)["id"]
    br = jrun(home, ["branch", "create", "main", "--thread", th], project)["id"]
    br_rev = jrun(home, ["branch", "list", "--thread", th], project)[-1]["head_sequence"]
    mem = jrun(home, ["memory", "commit", "--workspace", ws, "--type", "fact",
                      "--content", "SQLite es la fuente", "--scope", "workspace"], project)["id"]
    old = jrun(home, ["memory", "commit", "--workspace", ws, "--type", "fact",
                      "--content", "PostgreSQL era la fuente", "--scope", "workspace"], project)["id"]
    run(home, ["memory", "commit", "--workspace", ws, "--type", "fact",
               "--content", "SQLite manda", "--scope", "workspace", "--supersedes", old], project)

    art_text = jrun(home, ["artifact", "put", str(project / "nota.txt"), "--workspace", ws], project)
    art_bin = jrun(home, ["artifact", "put", str(project / "bin.dat"), "--workspace", ws], project)

    refs = write_refs(project / "refs.json", [
        {"reference_type": "file", "stable_id": "a.txt", "display_text": "a.txt",
         "content_hash": sha(project / "a.txt"), "selection_reason": "user_selected",
         "selected_at": "1700000000"},
        {"reference_type": "memory", "stable_id": mem, "display_text": "SQLite",
         "selection_reason": "user_selected", "selected_at": "1700000001"},
        {"reference_type": "artifact", "stable_id": art_text["id"], "display_text": "nota",
         "content_hash": art_text.get("sha256"), "selection_reason": "user_selected",
         "selected_at": "1700000002"},
        {"reference_type": "branch", "stable_id": "main", "display_text": "main",
         "revision": br_rev, "selection_reason": "user_selected", "selected_at": "1700000003"},
    ])
    run_args = ["run", "start", "--workspace", ws, "--thread", th, "--branch", br,
                "--prompt", "usa referencias", "--provider", "sim",
                "--explicit-references-file", str(refs)]
    run1 = jrun(home, run_args, project)
    ctx = run1["receipt"]["context_manifest"]
    manifest = jrun(home, ["manifest", "get", ctx], project)
    manifest = manifest.get("manifest", manifest)
    entries = manifest.get("explicit_references", [])
    types = {e["reference_type"] for e in entries}
    result["file"] = run1["state"] == "completed" and "file" in types
    result["memory"] = "memory" in types
    result["artifact"] = "artifact" in types
    result["branch"] = "branch" in types
    result["payload_hash"] = bool(manifest.get("payload_hash"))

    refs_run = write_refs(project / "refs-run.json", [
        {"reference_type": "run", "stable_id": run1["id"], "display_text": "run previo",
         "selection_reason": "user_selected", "selected_at": "1700000004"}])
    run_run = jrun(home, ["run", "start", "--workspace", ws, "--thread", th, "--branch", br,
                          "--prompt", "referencia a run", "--provider", "sim",
                          "--explicit-references-file", str(refs_run)], project)
    m2 = jrun(home, ["manifest", "get", run_run["receipt"]["context_manifest"]], project)
    m2 = m2.get("manifest", m2)
    result["run"] = any(
        e["reference_type"] == "run" and e.get("materialized_text")
        for e in m2.get("explicit_references", [])
    )

    refs_bin = write_refs(project / "refs-bin.json", [
        {"reference_type": "artifact", "stable_id": art_bin["id"], "display_text": "bin",
         "content_hash": art_bin.get("sha256"), "selection_reason": "user_selected",
         "selected_at": "1700000005"}])
    run_bin = jrun(home, ["run", "start", "--workspace", ws, "--thread", th, "--branch", br,
                          "--prompt", "binario", "--provider", "sim",
                          "--explicit-references-file", str(refs_bin)], project)
    mb = jrun(home, ["manifest", "get", run_bin["receipt"]["context_manifest"]], project)
    mb = mb.get("manifest", mb)
    e_bin = mb["explicit_references"][0]
    result["binary_safe"] = (
        e_bin["materialization_kind"] == "metadata_only"
        and e_bin.get("materialized_text") is None
    )

    used = run1["receipt"].get("used_explicit_reference_ids", [])
    result["receipt"] = mem in used and "a.txt" in used

    (project / "a.txt").write_text("contenido cambiado\n", encoding="utf-8")
    p = run(home, run_args, project, expect_fail=True)
    result["changed_detected"] = p.returncode != 0 and "reference_changed" in (p.stderr + p.stdout)
    refs2 = write_refs(project / "refs2.json", [
        {"reference_type": "file", "stable_id": "a.txt", "content_hash": sha(project / "a.txt"),
         "selection_reason": "user_selected", "selected_at": "1700000006"}])
    run2 = jrun(home, ["run", "start", "--workspace", ws, "--thread", th, "--branch", br,
                       "--prompt", "refrescada", "--provider", "sim",
                       "--explicit-references-file", str(refs2)], project)
    result["refresh"] = run2["state"] == "completed"

    missing = write_refs(project / "refs-missing.json", [
        {"reference_type": "file", "stable_id": "nope.txt", "selection_reason": "user_selected"}])
    p_missing = run(home, ["run", "start", "--workspace", ws, "--thread", th, "--branch", br,
                           "--prompt", "missing", "--provider", "sim",
                           "--explicit-references-file", str(missing)], project, expect_fail=True)
    sup = write_refs(project / "refs-sup.json", [
        {"reference_type": "memory", "stable_id": old, "selection_reason": "user_selected"}])
    p_sup = run(home, ["run", "start", "--workspace", ws, "--thread", th, "--branch", br,
                       "--prompt", "superseded", "--provider", "sim",
                       "--explicit-references-file", str(sup)], project, expect_fail=True)
    bad_rev = write_refs(project / "refs-rev.json", [
        {"reference_type": "branch", "stable_id": "main", "revision": 999,
         "selection_reason": "user_selected"}])
    p_rev = run(home, ["run", "start", "--workspace", ws, "--thread", th, "--branch", br,
                       "--prompt", "rev", "--provider", "sim",
                       "--explicit-references-file", str(bad_rev)], project, expect_fail=True)
    result["invalid_rejected"] = (
        p_missing.returncode != 0
        and p_sup.returncode != 0
        and p_rev.returncode != 0
        and "reference_" in (p_missing.stderr + p_sup.stderr + p_rev.stderr)
    )

    run(home, ["daemon", "stop"], expect_fail=True)
    run(home, ["daemon", "start"])
    again = jrun(home, ["manifest", "get", ctx], project)
    again = again.get("manifest", again)
    run_again = jrun(home, ["run", "get", run1["id"]], project)
    result["restart"] = (
        len(again.get("explicit_references", [])) == 4
        and mem in run_again["run"]["receipt"].get("used_explicit_reference_ids", [])
    )

    archive = project / "ws.hliar"
    run(home, ["export", ws, "--out", str(archive)], project)
    home2 = Path(tempfile.mkdtemp(prefix="hli-ref-import-"))
    run(home2, ["daemon", "start"])
    run(home2, ["import", str(archive)], project)
    imported = jrun(home2, ["manifest", "get", ctx], project)
    imported = imported.get("manifest", imported)
    imported_run = jrun(home2, ["run", "get", run1["id"]], project)
    result["export_import"] = (
        len(imported.get("explicit_references", [])) == 4
        and mem in imported_run["run"]["receipt"].get("used_explicit_reference_ids", [])
    )

    result["ok"] = all(
        result[k]
        for k in ("file", "artifact", "memory", "run", "branch", "payload_hash",
                  "changed_detected", "refresh", "invalid_rejected", "binary_safe",
                  "receipt", "restart", "export_import")
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
