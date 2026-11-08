#!/usr/bin/env python3
"""Benchmark v2: baseline vs HLI v1 (sin slices) vs HLI v2 (con slices).
3 repeticiones por tarea y modo. Routed: not_evaluable (un solo modelo real).
"""
import json
import os
import shutil
import statistics
import subprocess
import tempfile
import time
import urllib.request

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
HLI = os.path.join(ROOT, "target", "debug", "hli.exe")
DAEMON = os.path.join(ROOT, "target", "debug", "hli-daemon.exe")
OLLAMA = "http://127.0.0.1:11434/api/chat"
MODEL = "gemma4:31b-cloud"
PRICE_IN = 0.27 / 1_000_000
PRICE_OUT = 1.10 / 1_000_000
REPS = 3

LOG = "\n".join(
    [f"2026-11-07T10:{i:02d}:00Z INFO request id={i} path=/api/x status=200" for i in range(80)]
    + ["2026-11-07T10:40:00Z ERROR db timeout after 30s pool=8",
       "2026-11-07T10:41:00Z INFO retry ok"]
    + [f"2026-11-07T10:{i}:00Z INFO done id={i}" for i in range(42, 80)]
)

TASKS = [
    {"id": "log", "prompt": "¿Qué error grave aparece en el log?", "ctx": LOG,
     "artifact": True, "kind": "line_range",
     "check": lambda t: "db timeout" in t.lower()},
    {"id": "memory", "prompt": "¿Qué base de datos es la fuente de verdad?",
     "ctx": "SQLite es la fuente de verdad del proyecto.", "artifact": False,
     "check": lambda t: "sqlite" in t.lower()},
    {"id": "files", "prompt": "Nombra los tres archivos del workspace.",
     "ctx": "Archivos: a.rs, b.rs y c.rs.", "artifact": True, "kind": "full_text_if_small",
     "check": lambda t: all(x in t for x in ["a.rs", "b.rs", "c.rs"])},
    {"id": "fix", "prompt": "Corrige el bug: if (x = 1) { y(); }",
     "ctx": "El bug es asignación en lugar de comparación; debe ser ==.", "artifact": True,
     "kind": "full_text_if_small",
     "check": lambda t: "==" in t},
    {"id": "integrate", "prompt": "Integra en una frase: el build pasa y los tests fallan.",
     "ctx": "build ok; tests fallan.", "artifact": True, "kind": "full_text_if_small",
     "check": lambda t: "build" in t.lower() and "test" in t.lower()},
]


def run(env, *args):
    p = subprocess.run([HLI, *args], env=env, capture_output=True, text=True)
    if p.returncode != 0:
        raise RuntimeError(f"{args}: {p.stderr or p.stdout}")
    return json.loads(p.stdout) if p.stdout.strip().startswith(("{", "[")) else p.stdout


def ollama(prompt, max_tokens=160):
    body = json.dumps({"model": MODEL, "stream": False,
                       "options": {"temperature": 0, "num_predict": max_tokens},
                       "messages": [{"role": "user", "content": prompt}]}).encode()
    req = urllib.request.Request(OLLAMA, data=body, headers={"Content-Type": "application/json"})
    t0 = time.time()
    with urllib.request.urlopen(req, timeout=120) as r:
        v = json.loads(r.read())
    return {"text": v.get("message", {}).get("content", ""),
            "input": v.get("prompt_eval_count", 0), "output": v.get("eval_count", 0),
            "latency": int((time.time() - t0) * 1000)}


def main():
    base = tempfile.mkdtemp(prefix="hli-bench2-")
    env = dict(os.environ, HLI_HOME=os.path.join(base, "home"))
    os.makedirs(env["HLI_HOME"], exist_ok=True)
    open(os.path.join(env["HLI_HOME"], "prices.yaml"), "w").write(
        "provider: ollama\nmodel: gemma4:31b\ncurrency: USD\ninput_per_million: 0\n"
        "output_per_million: 0\neffective_from: 2026-11-07\nsource: user-configured\n")
    proc = subprocess.Popen([DAEMON], env=env, stdin=subprocess.DEVNULL,
                            stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    for _ in range(100):
        time.sleep(0.05)
        if subprocess.run([HLI, "daemon", "status"], env=env, capture_output=True).returncode == 0:
            break
    rows = []
    try:
        ws = run(env, "workspace", "create", "bench2", "--root", base)["id"]
        th = run(env, "thread", "create", "bench2", "--workspace", ws)["id"]
        br = run(env, "branch", "create", "main", "--thread", th)["id"]
        artifacts = {}
        for t in TASKS:
            run(env, "memory", "commit", "--workspace", ws, "--type", "fact", "--content", t["ctx"][:500])
            if t["artifact"]:
                path = os.path.join(base, f"{t['id']}.txt")
                open(path, "w", encoding="utf-8").write(t["ctx"])
                aid = run(env, "artifact", "put", path, "--workspace", ws)["id"]
                art = run(env, "artifact", "slice", aid, "--workspace", ws,
                          "--kind", t.get("kind", "full_text_if_small"),
                          "--reason", f"evidence for {t['id']}")
                artifacts[t["id"]] = aid
        for t in TASKS:
            for rep in range(REPS):
                # baseline
                text = f"Contexto:\n{t['ctx']}\n\nTarea: {t['prompt']}"
                r = ollama(text)
                ok = t["check"](r["text"])
                retries = 0
                inp, out, lat = r["input"], r["output"], r["latency"]
                if not ok:
                    retries = 1
                    r2 = ollama(f"Tu respuesta no cumple. Contexto:\n{t['ctx']}\nTarea: {t['prompt']}")
                    inp += r2["input"]; out += r2["output"]; lat += r2["latency"]
                rows.append({"task": t["id"], "mode": "baseline", "rep": rep, "input": inp,
                             "output": out, "latency": lat, "ok": ok, "retries": retries,
                             "slices": 0, "memories": 0,
                             "cost": inp * PRICE_IN + out * PRICE_OUT})
                # hli v1 / v2
                for mode, use_slices in (("hli_v1", False), ("hli_v2", True)):
                    args = ["run", "start", "--workspace", ws, "--thread", th, "--branch", br,
                            "--prompt", t["prompt"], "--provider", "ollama", "--model", MODEL,
                            "--max-tokens", "160"]
                    if use_slices and t["artifact"]:
                        args += ["--artifact", artifacts[t["id"]]]
                    t0 = time.time()
                    rv = run(env, *args)
                    got = run(env, "run", "get", rv["id"])
                    u = got["run"]["receipt"]["usage"]
                    text_out = got["run"]["result"].get("text", "")
                    ok = t["check"](text_out)
                    retries = 0
                    inp = u["input_tokens"]; out = u["output_tokens"]
                    if not ok:
                        retries = 1
                        rv2 = run(env, *args)
                        got2 = run(env, "run", "get", rv2["id"])
                        u2 = got2["run"]["receipt"]["usage"]
                        inp += u2["input_tokens"]; out += u2["output_tokens"]
                    rows.append({"task": t["id"], "mode": mode, "rep": rep, "input": inp,
                                 "output": out, "latency": int((time.time() - t0) * 1000),
                                 "ok": ok, "retries": retries,
                                 "slices": len(got["manifest"]["artifact_slices"]),
                                 "memories": len(got["manifest"]["memories"]),
                                 "cost": inp * PRICE_IN + out * PRICE_OUT})
    finally:
        subprocess.run([HLI, "daemon", "stop"], env=env, capture_output=True)
        proc.wait(timeout=5)
        shutil.rmtree(base, ignore_errors=True)

    modes = ["baseline", "hli_v1", "hli_v2"]
    lines = ["# HLI — Benchmark v2", "",
             f"Modelo: `{MODEL}` vía Ollama. {REPS} repeticiones por tarea y modo. "
             f"Tarifa de referencia: ${PRICE_IN*1e6:.2f}/M input, ${PRICE_OUT*1e6:.2f}/M output.", "",
             "Routed: **not_evaluable** — no hay un segundo modelo generativo distinto y más barato.", "",
             "| Tarea | Modo | Input | Output | Latencia ms | Calidad | Reintentos | Slices | Memorias | Coste ref. |",
             "|---|---|---|---|---|---|---|---|---|---|"]
    for t in [x["id"] for x in TASKS]:
        for mode in modes:
            sel = [r for r in rows if r["task"] == t and r["mode"] == mode]
            if not sel:
                continue
            q = sum(1 for r in sel if r["ok"])
            lines.append(f"| {t} | {mode} | {int(statistics.mean(r['input'] for r in sel))} | "
                         f"{int(statistics.mean(r['output'] for r in sel))} | "
                         f"{int(statistics.mean(r['latency'] for r in sel))} | {q}/{len(sel)} | "
                         f"{sum(r['retries'] for r in sel)} | {int(statistics.mean(r['slices'] for r in sel))} | "
                         f"{int(statistics.mean(r['memories'] for r in sel))} | "
                         f"${sum(r['cost'] for r in sel):.6f} |")
    lines.append("")
    for mode in modes:
        sel = [r for r in rows if r["mode"] == mode]
        lines.append(f"- **{mode}**: input medio {int(statistics.mean(r['input'] for r in sel))}, "
                     f"calidad {sum(1 for r in sel if r['ok'])}/{len(sel)}, "
                     f"reintentos {sum(r['retries'] for r in sel)}, "
                     f"coste total ${sum(r['cost'] for r in sel):.6f}, "
                     f"latencia media {int(statistics.mean(r['latency'] for r in sel))} ms")
    open(os.path.join(ROOT, "docs", "BENCHMARK-REPORT-v2.md"), "w", encoding="utf-8").write("\n".join(lines) + "\n")
    print(json.dumps({"ok": True, "rows": len(rows), "report": "docs/BENCHMARK-REPORT-v2.md"}))


if __name__ == "__main__":
    main()
