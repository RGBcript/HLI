#!/usr/bin/env python3
"""Gate de firma Ed25519 de módulos: clave, trust, firma, instalación, tamper y revocación."""
import hashlib
import json
import os
import shutil
import subprocess
import sys
import tempfile
import time

ROOT = r"C:\Users\GUERRA\Documents\TEC\HLI"
DEBUG = os.path.join(ROOT, "target", "debug")
HLI = os.path.join(DEBUG, "hli.exe")
DAEMON = os.path.join(DEBUG, "hli-daemon.exe")
FIXTURE = os.path.join(DEBUG, "hli-module-fixture.exe")
MODULE = "hli-module-fixture"


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
            "operations": [{"name": "memory.search", "effects": "read"}],
            "permissions": {"filesystem": "none", "network": "daemon-only", "secrets": "none"},
            "lifecycle": {"health": "health", "shutdown": "shutdown"},
        },
    }
    manifest_bytes = json.dumps(manifest).encode()
    binary = open(FIXTURE, "rb").read()
    checks = {"manifest.json": sha(manifest_bytes), "bin/hli-module-fixture.exe": sha(binary)}
    raw = container([
        ("manifest.json", manifest_bytes),
        ("bin/hli-module-fixture.exe", binary),
        ("checksums.json", json.dumps(checks).encode()),
    ])
    open(dest, "wb").write(raw)


def run(env, *args, check=True):
    p = subprocess.run([HLI, *args], env=env, capture_output=True, text=True)
    if check and p.returncode != 0:
        raise RuntimeError(f"{args}: {p.stderr}{p.stdout}")
    return p


def jrun(env, *args):
    return json.loads(run(env, *args).stdout)


def start_daemon(env):
    d = subprocess.Popen(
        [DAEMON], env=env, stdin=subprocess.DEVNULL,
        stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
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


def tamper(src, dest):
    raw = bytearray(open(src, "rb").read())
    marker = b"hli-module-fixture"
    # Alterar un byte del binario empaquetado (tras la cabecera de la segunda sección).
    idx = raw.find(b"MZ")
    if idx < 0:
        raise RuntimeError("no se encontró el binario en el paquete")
    raw[idx + 1] ^= 0xFF
    open(dest, "wb").write(raw)


def leak_scan(home, secret, outputs):
    for text in outputs:
        if secret in text:
            return True
    for base, _, files in os.walk(home):
        if os.path.basename(base) == "keys":
            files[:] = [f for f in files if not f.endswith(".key")]
        for name in files:
            path = os.path.join(base, name)
            try:
                if secret in open(path, encoding="utf-8", errors="ignore").read():
                    return True
            except Exception:
                continue
    return False


def main():
    home = tempfile.mkdtemp(prefix="hli-signing-gate-")
    env = dict(os.environ, HLI_HOME=home, HLI_MODULE_POLICY="require-signed")
    out = {"gate": "module-signing", "ok": False}
    d = None
    phase = "arranque"
    outputs = []
    secret = ""
    try:
        phase = "clave"
        key = jrun(env, "key", "generate", "gate-key")
        key_id = key["key_id"]
        secret = open(os.path.join(home, "keys", f"{key_id}.key"), encoding="utf-8").read().strip()
        secret = secret[len("base64:"):] if secret.startswith("base64:") else secret
        pub_path = os.path.join(home, "gate-public.json")
        public = jrun(env, "key", "export-public", key_id)
        open(pub_path, "w", encoding="utf-8").write(json.dumps(public))
        trust = jrun(env, "trust", "add", pub_path)
        if trust["key_id"] != key_id:
            raise RuntimeError("trust add devolvió otro key_id")
        if jrun(env, "trust", "list")[0]["revoked_at"] is not None:
            raise RuntimeError("la clave no debería estar revocada")

        phase = "firma"
        pkg = os.path.join(home, "fixture.hlimod")
        make_package(pkg)
        jrun(env, "module", "sign", pkg, "--key", key_id)
        verified = jrun(env, "module", "verify", pkg)
        signed = verified["state"] == "signed-trusted"
        trusted = verified["accepted"] is True and verified["policy"] == "require-signed"

        phase = "instalación"
        d = start_daemon(env)
        installed = jrun(env, "module", "install", pkg)
        if installed["signature_state"] != "signed-trusted":
            raise RuntimeError(f"instalación no confiada: {installed}")
        ws = jrun(env, "workspace", "create", "gate")["id"]
        jrun(env, "mch", "create", "signed-main", "--module", MODULE, "--workspace", ws)
        jrun(env, "mch", "serve", "signed-main")
        invoked = jrun(env, "mch", "invoke", "signed-main", "memory.search",
                       "--arguments", '{"query":""}')
        if invoked["result"]["pid"] is None:
            raise RuntimeError("el módulo firmado no ejecutó")

        phase = "tamper"
        bad = os.path.join(home, "tampered.hlimod")
        tamper(pkg, bad)
        bad_verify = run(env, "module", "verify", bad, check=False)
        outputs.append(bad_verify.stdout + bad_verify.stderr)
        tamper_rejected = bad_verify.returncode != 0
        if bad_verify.returncode == 0:
            tamper_rejected = json.loads(bad_verify.stdout)["state"] == "invalid-signature"
        install_bad = run(env, "module", "install", bad, check=False)
        outputs.append(install_bad.stdout + install_bad.stderr)
        if install_bad.returncode == 0:
            tamper_rejected = False

        phase = "revocación"
        jrun(env, "trust", "revoke", key_id)
        rejected = run(env, "module", "install", pkg, check=False)
        outputs.append(rejected.stdout + rejected.stderr)
        revocation_enforced = rejected.returncode != 0 and (
            "signature_rejected" in rejected.stderr or "revocad" in rejected.stderr
        )
        modules = jrun(env, "module", "list")
        installed_entry = next((m for m in modules if m["name"] == MODULE), None)
        if installed_entry is None or installed_entry["signature_state"] != "signed-revoked":
            raise RuntimeError(f"el módulo instalado no quedó marcado: {modules}")
        status = jrun(env, "mch", "status", "signed-main")
        if status["instance"]["state"] != "ready":
            raise RuntimeError(f"el módulo ya instalado perdió su estado: {status}")

        phase = "fuga"
        outputs.append(run(env, "key", "list").stdout)
        outputs.append(run(env, "trust", "list").stdout)
        outputs.append(run(env, "module", "list").stdout)
        outputs.append(open(pkg, "rb").read().decode("utf-8", errors="ignore"))
        private_key_leak = leak_scan(home, secret, outputs)

        out.update({
            "signed": bool(signed),
            "trusted": bool(trusted),
            "tamper_rejected": bool(tamper_rejected),
            "revocation_enforced": bool(revocation_enforced),
            "private_key_leak": bool(private_key_leak),
        })
        out["ok"] = all([
            out["signed"], out["trusted"], out["tamper_rejected"],
            out["revocation_enforced"], not out["private_key_leak"],
        ])
        print(json.dumps(out, indent=2))
    except Exception as exc:
        print(json.dumps({**out, "ok": False, "phase": phase, "error": str(exc)}, indent=2))
        return 1
    finally:
        if d is not None:
            stop_daemon(env, d)
        shutil.rmtree(home, ignore_errors=True)
    return 0 if out["ok"] else 1


if __name__ == "__main__":
    sys.exit(main())
