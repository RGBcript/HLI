#!/usr/bin/env python3
"""Empaqueta modules/hli-memory en dist/hli-memory-<version>.hlimod."""
import hashlib
import json
import os

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
MOD = os.path.join(ROOT, "modules", "hli-memory")
BIN = os.path.join(ROOT, "target", "debug", "hli-memory.exe")


def sha(b):
    return "sha256:" + hashlib.sha256(b).hexdigest()


def container(files):
    out = b"HLIAR1\n"
    for path, data in files:
        out += f"FILE {len(data)} {path}\n".encode() + data + b"\n"
    return out


def main():
    manifest = json.load(open(os.path.join(MOD, "manifest.json"), encoding="utf-8"))
    version = manifest["metadata"]["version"]
    files = [("manifest.json", json.dumps(manifest).encode())]
    schemas = os.path.join(MOD, "schemas")
    for name in sorted(os.listdir(schemas)):
        files.append((f"schemas/{name}", open(os.path.join(schemas, name), "rb").read()))
    files.append(("bin/hli-memory.exe", open(BIN, "rb").read()))
    checks = {p: sha(d) for p, d in files}
    files.append(("checksums.json", json.dumps(checks).encode()))
    os.makedirs(os.path.join(ROOT, "dist"), exist_ok=True)
    out = os.path.join(ROOT, "dist", f"hli-memory-{version}.hlimod")
    open(out, "wb").write(container(files))
    print(out)


if __name__ == "__main__":
    main()
