#!/usr/bin/env python3
"""Gate de instalación: binarios juntos, doctor, PATH y primera ejecución.

Cubre lo que promete `install.ps1`/`install.sh` sin tocar el perfil real:
instala en un prefijo temporal, arranca el daemon con un HLI_HOME temporal y
ejecuta una tarea en un directorio sin workspace previo.
"""
import json
import os
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
IS_WIN = os.name == "nt"
BINS = ["hli", "hli-tui", "hli-daemon", "hli-mcp"]
EXE = ".exe" if IS_WIN else ""

def run(cmd, cwd=None, env=None, timeout=120):
    return subprocess.run(
        cmd, cwd=cwd, env=env, capture_output=True, text=True, timeout=timeout
    )

def main():
    result = {"gate": "install", "ok": False}
    work = Path(tempfile.mkdtemp(prefix="hli-install-gate-"))
    prefix = work / "bin"
    project = work / "project"
    home = work / "home"
    project.mkdir()
    home.mkdir()

    try:
        result["installer_present"] = (ROOT / "install.ps1").exists() and (
            ROOT / "install.sh"
        ).exists()
        result["docs_present"] = (ROOT / "docs" / "INSTALL.md").exists()

        release = ROOT / "target" / "release"
        result["release_binaries"] = all(
            (release / f"{b}{EXE}").exists() for b in BINS
        )
        if not result["release_binaries"]:
            result["error"] = "faltan binarios release; ejecuta cargo build --release --workspace"
            return finish(result)

        prefix.mkdir()
        for b in BINS:
            shutil.copy2(release / f"{b}{EXE}", prefix / f"{b}{EXE}")
        result["install_copies_binaries"] = all(
            (prefix / f"{b}{EXE}").exists() for b in BINS
        )

        env = dict(os.environ)
        env["HLI_HOME"] = str(home)
        env["PATH"] = str(prefix) + os.pathsep + env.get("PATH", "")
        # Aísla de un daemon real del usuario.
        for key in ("HLI_DAEMON_PORT",):
            env.pop(key, None)

        hli = prefix / f"hli{EXE}"

        version = run([str(hli), "--version"], env=env)
        result["version_ok"] = version.returncode == 0 and "hli " in version.stdout

        doctor = run([str(hli), "doctor", "--json"], env=env)
        try:
            report = json.loads(doctor.stdout)
        except json.JSONDecodeError:
            report = {}
        result["doctor_ok"] = bool(report.get("ok"))
        result["doctor_path"] = (report.get("path") or {}).get("present") is True
        result["doctor_binaries"] = all(
            b.get("present") for b in report.get("binaries", [])
        )

        # Primera ejecución en un directorio sin workspace: debe crear el
        # proyecto, arrancar el daemon y devolver respuesta.
        task = run(
            [str(hli), "-p", "di hola", "--output", "human"],
            cwd=project,
            env=env,
            timeout=180,
        )
        result["no_workspace_task_ok"] = task.returncode == 0 and bool(
            task.stdout.strip()
        )
        result["no_workspace_task_output"] = task.stdout.strip()[:120]

        # El workspace quedó vinculado al directorio.
        again = run([str(hli), "-p", "otra vez", "--output", "human"], cwd=project, env=env, timeout=180)
        result["second_task_reuses_project"] = again.returncode == 0

        listed = run([str(hli), "workspace", "list"], cwd=project, env=env)
        try:
            spaces = json.loads(listed.stdout)
            roots = [w.get("root_path", "") for w in spaces]
        except (json.JSONDecodeError, TypeError):
            roots = []
        result["workspace_linked"] = any(
            str(project).lower().replace("\\", "/") in str(r).lower().replace("\\", "/")
            for r in roots
        )

        stop = run([str(hli), "daemon", "stop"], env=env)
        result["daemon_stop_ok"] = stop.returncode == 0
    finally:
        shutil.rmtree(work, ignore_errors=True)

    return finish(result)

def finish(result):
    checks = [k for k in result if k.startswith(("install", "docs", "release", "version", "doctor", "no_workspace", "second", "workspace", "daemon"))]
    result["ok"] = all(result[k] for k in checks if isinstance(result[k], bool))
    print(json.dumps(result, indent=2))
    return 0 if result["ok"] else 1

if __name__ == "__main__":
    sys.exit(main())
