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


def wait_for(env, predicate, timeout=8.0, interval=0.05):
    end = time.time() + timeout
    last = None
    while time.time() < end:
        last = status(env)
        if predicate(last):
            return last
        time.sleep(interval)
    raise RuntimeError(f"timeout; último estado: {json.dumps(last)}")


def wait_pid_change(env, old_pid, timeout=20.0):
    return wait_for(
        env,
        lambda s: s["process"] is not None
        and s["process"]["pid"] is not None
        and s["process"]["pid"] != old_pid,
        timeout=timeout,
        interval=0.02,
    )


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
        HLI_MCH_RESTART_WINDOW_MS="60000",
    )
    out = {"gate": "mch-supervisor", "ok": False}
    d = None
    phase = "arranque"
    try:
        d = start_daemon(env)
        ws = jrun(env, "workspace", "create", "gate")["id"]
        run(env, "module", "install", pkg)
        run(env, "mch", "create", INSTANCE, "--module", MODULE,
            "--domain", "memory", "--workspace", ws)

        phase = "lifecycle"
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

        phase = "degraded"
        write_fixture(home, {"mode": "recover_after_n", "health_failures": 2})
        degraded = wait_for(
            env, lambda s: s["process"] is not None
            and s["process"]["state"] == "degraded"
        )
        out["degraded"] = degraded["process"]["health_failures"] >= 2

        phase = "recovered"
        write_fixture(home, {"mode": "normal"})
        wait_for(
            env, lambda s: s["process"] is not None
            and s["process"]["state"] == "ready"
            and s["process"]["health_failures"] == 0
        )
        out["recovered"] = True

        phase = "restart básico"
        write_fixture(home, {"mode": "health_fail_n", "health_failures": 999999,
                             "skip_first_health": True})
        restarted = wait_pid_change(env, pid1, timeout=15)
        pid2 = restarted["process"]["pid"]
        out["restarted"] = pid2 != pid1 and restarted["process"]["generation"] > 1
        write_fixture(home, {"mode": "normal"})
        wait_for(env, lambda s: s["process"] is not None
                 and s["process"]["state"] == "ready")

        phase = "límite de reinicios"
        pid3 = jrun(env, "mch", "restart", INSTANCE)["pid"]
        write_fixture(home, {"mode": "health_fail_n", "health_failures": 999999,
                             "skip_first_health": True})
        auto_pids = []
        seen = pid3
        for _ in range(3):
            st = wait_pid_change(env, seen, timeout=25)
            auto_pids.append(st["process"]["pid"])
            seen = auto_pids[-1]
        three_pids = len(set(auto_pids)) == 3 and all(
            p not in (pid1, pid2, pid3) for p in auto_pids
        )

        failed = wait_for(
            env, lambda s: s["process"] is not None
            and s["process"]["state"] == "failed",
            timeout=25,
        )
        failed_gen = failed["process"]["generation"]
        limit_reached = (
            failed["process"]["pid"] is None
            and failed["process"]["restart_count_in_window"] == 3
            and failed["process"]["last_error"]["code"] == "restart_limit_exceeded"
        )

        time.sleep(1.0)
        again = status(env)
        no_new_pid = (
            again["process"] is not None
            and again["process"]["state"] == "failed"
            and again["process"]["pid"] is None
            and again["process"]["generation"] == failed_gen
        )
        daemon_alive = jrun(env, "daemon", "status")["running"] and isinstance(
            jrun(env, "workspace", "list"), list
        )

        phase = "recuperación manual"
        write_fixture(home, {"mode": "normal"})
        jrun(env, "mch", "restart", INSTANCE)
        ready = wait_for(
            env, lambda s: s["process"] is not None
            and s["process"]["state"] == "ready"
        )
        manual_recovery = (
            ready["process"]["generation"] == failed_gen + 1
            and ready["process"]["restart_count_in_window"] == 0
            and ready["process"]["restart_window_started_at"] is None
            and ready["process"]["next_restart_at"] is None
        )
        out["restart_limited"] = bool(
            three_pids and limit_reached and no_new_pid and daemon_alive
            and manual_recovery
        )

        phase = "autostart"
        jrun(env, "mch", "set-autostart", INSTANCE, "true")
        jrun(env, "memory", "commit", "--workspace", ws, "--type", "fact",
             "--content", "memoria persistente del gate")
        stop_daemon(env, d)
        d = start_daemon(env)
        auto = wait_for(
            env, lambda s: s["process"] is not None
            and s["process"]["state"] == "ready"
            and s["process"]["alive"],
            timeout=20,
        )
        out["autostart"] = auto["process"]["pid"] is not None

        phase = "memoria"
        found = jrun(env, "memory", "search", "--workspace", ws,
                     "--query", "persistente")
        if "memoria persistente del gate" not in json.dumps(found, ensure_ascii=False):
            raise RuntimeError(f"memoria no recuperada: {found}")

        phase = "cierre"
        stopped = jrun(env, "mch", "stop", INSTANCE)
        out["clean_closure"] = stopped["state"] == "stopped"

        out["ok"] = all([
            out["lifecycle"],
            out["persistent_process"],
            out["same_pid"],
            out["degraded"],
            out["recovered"],
            out["restarted"],
            out["restart_limited"],
            out["autostart"],
            out["clean_closure"],
        ])
        print(json.dumps(out, indent=2))
    except Exception as exc:
        print(json.dumps({**out, "ok": False, "phase": phase, "error": str(exc)},
                         indent=2))
        return 1
    finally:
        if d is not None:
            stop_daemon(env, d)
        shutil.rmtree(home, ignore_errors=True)
    return 0 if out["ok"] else 1


if __name__ == "__main__":
    sys.exit(main())
