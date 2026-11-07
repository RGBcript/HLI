#!/usr/bin/env python3
"""Gate externo del supervisor MCH. Usa CLI, daemon, instalador y .hlimod reales."""
import hashlib
import json
import os
import shutil
import subprocess
import sys
import tempfile
import time

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
HLI = os.path.join(ROOT, "target", "debug", "hli.exe")
FIXTURE = os.path.join(ROOT, "target", "debug", "hli-module-fixture.exe")
MODULE = "hli-module-fixture"
INSTANCE = "fixture-main"


def sha(data):
    return "sha256:" + hashlib.sha256(data).hexdigest()


def container(files):
    out = b"HLIAR1\n"
    for path, data in files:
        out += f"FILE {len(data)} {path}\n".encode() + data + b"\n"
    return out


def make_package(dest):
    manifest = {
        "apiVersion": "hli.dev/v1",
        "kind": "Module",
        "metadata": {"name": MODULE, "version": "0.1.0"},
        "spec": {
            "sdk": ">=0.4 <0.5",
            "protocol": "hli-module/1",
            "transport": "stdio-ndjson",
            "runtime": {"kind": "subprocess", "entry": "bin/hli-module-fixture.exe"},
            "operations": [
                {"name": "memory.search", "effects": "read"},
                {"name": "memory.commit", "effects": "write"},
            ],
            "permissions": {"filesystem": "none", "network": "daemon-only", "secrets": "none"},
            "lifecycle": {"health": "health", "shutdown": "shutdown"},
        },
    }
    files = [("manifest.json", json.dumps(manifest).encode())]
    binary = open(FIXTURE, "rb").read()
    checks = {p: sha(d) for p, d in files}
    checks["bin/hli-module-fixture.exe"] = sha(binary)
    files.append(("bin/hli-module-fixture.exe", binary))
    files.append(("checksums.json", json.dumps(checks).encode()))
    open(dest, "wb").write(container(files))
    return dest


def run(env, *args, check=True):
    r = subprocess.run([HLI, *args], env=env, capture_output=True, text=True)
    if check and r.returncode != 0:
        raise RuntimeError(f"{args}: {r.stderr}{r.stdout}")
    return r


def jrun(env, *args):
    return json.loads(run(env, *args).stdout)


def start_daemon(env):
    d = subprocess.Popen(
        [os.path.join(ROOT, "target", "debug", "hli-daemon.exe")],
        env=env, stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
    )
    for _ in range(100):
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


def status(env):
    return jrun(env, "mch", "status", INSTANCE)


def wait_for(env, predicate, timeout=8.0):
    end = time.time() + timeout
    last = None
    while time.time() < end:
        last = status(env)
        if predicate(last):
            return last
        time.sleep(0.05)
    raise RuntimeError(f"timeout; último estado: {json.dumps(last)}")


def write_fixture(home, cfg):
    with open(os.path.join(home, "fixture.json"), "w", encoding="utf-8") as f:
        json.dump(cfg, f)


def main():
    home = tempfile.mkdtemp(prefix="hli-mch-gate-")
    pkg = os.path.join(home, "fixture.hlimod")
    make_package(pkg)
    env = dict(
        os.environ,
        HLI_HOME=home,
        HLI_MCH_HEALTH_INTERVAL_MS="100",
        HLI_MCH_HEALTH_TIMEOUT_MS="200",
        HLI_MCH_DEGRADED_AFTER="2",
        HLI_MCH_RESTART_AFTER="3",
        HLI_MCH_MAX_RESTARTS="3",
        HLI_MCH_BACKOFF_MS="50",
    )
    out = {"gate": "mch-supervisor", "ok": False}
    d = None
    try:
        d = start_daemon(env)
        ws = jrun(env, "workspace", "create", "gate")["id"]
        run(env, "module", "install", pkg)
        run(env, "mch", "create", INSTANCE, "--module", MODULE,
            "--domain", "memory", "--workspace", ws)

        write_fixture(home, {"mode": "normal"})
        served = jrun(env, "mch", "serve", INSTANCE)
        pid1 = served["pid"]
        out["lifecycle"] = True
        out["persistent_process"] = True

        inv1 = jrun(env, "mch", "invoke", INSTANCE, "memory.search",
                    "--arguments", '{"query":""}')
        inv2 = jrun(env, "mch", "invoke", INSTANCE, "memory.search",
                    "--arguments", '{"query":""}')
        out["same_pid"] = inv1["result"]["pid"] == inv2["result"]["pid"] == pid1

        write_fixture(home, {"mode": "recover_after_n", "health_failures": 2})
        degraded = wait_for(env, lambda s: s["process"]["state"] == "degraded")
        out["degraded"] = degraded["process"]["health_failures"] >= 2

        write_fixture(home, {"mode": "normal"})
        recovered = wait_for(env, lambda s: s["process"]["state"] == "ready"
                             and s["process"]["health_failures"] == 0)
        out["recovered"] = True

        write_fixture(home, {"mode": "health_fail_n", "health_failures": 99,
                             "skip_first_health": True})
        restarted = wait_for(
            env,
            lambda s: s["process"] is not None
            and s["process"]["pid"] != pid1
            and s["process"]["generation"] > 1,
            timeout=10,
        )
        pid2 = restarted["process"]["pid"]
        out["restarted"] = pid2 != pid1
        out["generation"] = restarted["process"]["generation"]

        write_fixture(home, {"mode": "normal"})
        wait_for(env, lambda s: s["process"] is not None
                 and s["process"]["state"] == "ready", timeout=8)

        stopped = jrun(env, "mch", "stop", INSTANCE)
        out["clean_closure"] = stopped["state"] == "stopped"
        out["autostart"] = False
        out["restart_limited"] = False
        out["ok"] = all([
            out["lifecycle"], out["persistent_process"], out["same_pid"],
            out["degraded"], out["recovered"], out["restarted"],
            out["clean_closure"], out["autostart"], out["restart_limited"],
        ])
        print(json.dumps(out, indent=2))
    finally:
        if d is not None:
            stop_daemon(env, d)
        shutil.rmtree(home, ignore_errors=True)
    return 0 if out["ok"] else 1


if __name__ == "__main__":
    sys.exit(main())
