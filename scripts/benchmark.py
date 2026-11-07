#!/usr/bin/env python3
"""Benchmark reproducible de HLI con Ollama real.

Tres modos: baseline (contexto completo), hli (ContextManifest + memoria) y
routed (dos llamadas: extracción + integración). Mide tokens reportados,
latencia, coste real y equivalente, reintentos y calidad determinista.
"""
import json
import os
import re
import shutil
import subprocess
import tempfile
import time

ROOT = r"C:\Users\GUERRA\Documents\TEC\HLI"
HLI = os.path.join(ROOT, "target", "debug", "hli.exe")
DAEMON = os.path.join(ROOT, "target", "debug", "hli-daemon.exe")
OLLAMA = "http://127.0.0.1:11434/api/chat"
MODEL = "gemma4:31b-cloud"
PRICE_IN = 0.27 / 1_000_000   # referencia de un modelo de pago, USD/token
PRICE_OUT = 1.10 / 1_000_000

LOG = "\n".join(
    [f"2026-11-07T10:{i:02d}:00Z INFO request id={i} path=/api/x status=200" for i in range(40)]
    + ["2026-11-07T10:40:00Z ERROR db timeout after 30s pool=8", "2026-11-07T10:41:00Z INFO retry ok"]
)

TASKS = [
    {"id": "log", "prompt": "¿Qué error grave aparece en el log y en qué línea?",
     "check": lambda t: "db timeout" in t.lower() and "ERROR" in t,
     "ctx": LOG},
    {"id": "memory", "prompt": "¿Qué base de datos es la fuente de verdad?",
     "check": lambda t: "sqlite" in t.lower(), "ctx": "SQLite es la fuente de verdad del proyecto."},
    {"id": "files", "prompt": "Nombra los tres archivos del workspace.",
     "check": lambda t: all(x in t for x in ["a.rs", "b.rs", "c.rs"]),
     "ctx": "Archivos: a.rs, b.rs y c.rs."},
    {"id": "fix", "prompt": "Corrige el bug: if (x = 1) { y(); }",
     "check": lambda t: "==" in t, "ctx": "El bug es asignación en lugar de comparación."},
    {"id": "integrate", "prompt": "Integra en una frase: el build pasa y los tests fallan.",
     "check": lambda t: "build" in t.lower() and ("test" in t.lower()), "ctx": "build ok; tests fallan."},
]


def run(env, *args):
    p = subprocess.run([HLI, *args], env=env, capture_output=True, text=True)
    if p.returncode != 0:
        raise RuntimeError(f"{args}: {p.stderr or p.stdout}")
    return json.loads(p.stdout) if p.stdout.strip().startswith(("{", "[")) else p.stdout


def ollama(prompt, max_tokens=120):
    body = json.dumps({"model": MODEL, "stream": False, "options": {"temperature": 0, "num_predict": max_tokens},
                       "messages": [{"role": "user", "content": prompt}]}).encode()
    import urllib.request
    req = urllib.request.Request(OLLAMA, data=body, headers={"Content-Type": "application/json"})
    t0 = time.time()
    with urllib.request.urlopen(req, timeout=120) as r:
        v = json.loads(r.read())
    return {
        "text": v.get("message", {}).get("content", ""),
        "input": v.get("prompt_eval_count", 0),
        "output": v.get("eval_count", 0),
        "latency": int((time.time() - t0) * 1000),
    }


def main():
    base = tempfile.mkdtemp(prefix="hli-bench-")
    env = dict(os.environ, HLI_HOME=os.path.join(base, "home"))
    os.makedirs(env["HLI_HOME"], exist_ok=True)
    open(os.path.join(env["HLI_HOME"], "prices.yaml"), "w").write(
        f"provider: ollama\nmodel: gemma4:31b\ncurrency: USD\ninput_per_million: 0\n"
        f"output_per_million: 0\neffective_from: 2026-11-07\nsource: user-configured\n")

    proc = subprocess.Popen([DAEMON], env=env, stdin=subprocess.DEVNULL,
                            stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    for _ in range(100):
        time.sleep(0.05)
        if subprocess.run([HLI, "daemon", "status"], env=env, capture_output=True).returncode == 0:
            break
    rows = []
    try:
        ws = run(env, "workspace", "create", "bench", "--root", base)["id"]
        th = run(env, "thread", "create", "bench", "--workspace", ws)["id"]
        br = run(env, "branch", "create", "main", "--thread", th)["id"]
        for t in TASKS:
            run(env, "memory", "commit", "--workspace", ws, "--type", "fact", "--content", t["ctx"][:400])
        # baseline
        for t in TASKS:
            prompt = f"Contexto:\n{t['ctx']}\n\nTarea: {t['prompt']}"
            r = ollama(prompt)
            ok = t["check"](r["text"])
            rows.append({"task": t["id"], "mode": "baseline", "input": r["input"], "output": r["output"],
                         "latency": r["latency"], "retries": 0 if ok else 1, "ok": ok,
                         "cost": r["input"] * PRICE_IN + r["output"] * PRICE_OUT})
        # hli
        for t in TASKS:
            r0 = time.time()
            runv = run(env, "run", "start", "--workspace", ws, "--thread", th, "--branch", br,
                       "--prompt", t["prompt"], "--provider", "ollama", "--model", MODEL,
                       "--max-tokens", "120")
            got = run(env, "run", "get", runv["id"])
            u = got["run"]["receipt"]["usage"]
            text = got["run"]["result"].get("text", "")
            ok = t["check"](text)
            rows.append({"task": t["id"], "mode": "hli", "input": u["input_tokens"],
                         "output": u["output_tokens"], "latency": int((time.time() - r0) * 1000),
                         "retries": 0 if ok else 1, "ok": ok,
                         "cost": u["input_tokens"] * PRICE_IN + u["output_tokens"] * PRICE_OUT})
        # routed: extracción + integración
        for t in TASKS:
            mid = ollama(f"Extrae los datos necesarios de: {t['ctx'][:400]}", 80)
            fin = ollama(f"Con estos datos: {mid['text']}\nResponde: {t['prompt']}", 120)
            text = fin["text"]
            ok = t["check"](text)
            rows.append({"task": t["id"], "mode": "routed",
                         "input": mid["input"] + fin["input"], "output": mid["output"] + fin["output"],
                         "latency": mid["latency"] + fin["latency"], "retries": 0 if ok else 1, "ok": ok,
                         "cost": (mid["input"] + fin["input"]) * PRICE_IN + (mid["output"] + fin["output"]) * PRICE_OUT})
    finally:
        subprocess.run([HLI, "daemon", "stop"], env=env, capture_output=True)
        proc.wait(timeout=5)
        shutil.rmtree(base, ignore_errors=True)

    # informe
    lines = ["# HLI — Benchmark de proveedores y coste", "",
             f"Modelo real: `{MODEL}` vía Ollama. Precio de referencia: ${PRICE_IN*1e6:.2f}/M input, "
             f"${PRICE_OUT*1e6:.2f}/M output (no es el precio real de Ollama, que es 0).", "",
             "| Tarea | Modo | Input | Output | Latencia ms | Reintentos | Calidad | Coste ref. USD |",
             "|---|---|---|---|---|---|---|---|"]
    for r in rows:
        lines.append(f"| {r['task']} | {r['mode']} | {r['input']} | {r['output']} | {r['latency']} | "
                     f"{r['retries']} | {'ok' if r['ok'] else 'fallo'} | {r['cost']:.6f} |")
    lines.append("")
    for mode in ["baseline", "hli", "routed"]:
        sel = [r for r in rows if r["mode"] == mode]
        lines.append(f"- **{mode}**: {sum(r['input'] for r in sel)} input, {sum(r['output'] for r in sel)} output, "
                     f"{sum(r['latency'] for r in sel)} ms, {sum(r['retries'] for r in sel)} reintentos, "
                     f"calidad {sum(1 for r in sel if r['ok'])}/{len(sel)}, coste ref. ${sum(r['cost'] for r in sel):.6f}")
    open(os.path.join(ROOT, "docs", "BENCHMARK-REPORT.md"), "w", encoding="utf-8").write("\n".join(lines) + "\n")
    print(json.dumps({"ok": True, "rows": len(rows), "report": "docs/BENCHMARK-REPORT.md"}))


if __name__ == "__main__":
    main()
