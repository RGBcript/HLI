#!/usr/bin/env python3
"""Benchmark v3 + ablaciones. Cargas grandes, 5 repeticiones, percentiles.
Routed usa qwen2.5-coder:1.5b (local) como secundario real y gemma4 como principal.
"""
import json
import os
import shutil
import statistics
import subprocess
import tempfile
import time
import urllib.request

ROOT = r"C:\Users\GUERRA\Documents\TEC\HLI"
HLI = os.path.join(ROOT, "target", "debug", "hli.exe")
DAEMON = os.path.join(ROOT, "target", "debug", "hli-daemon.exe")
OLLAMA = "http://127.0.0.1:11434/api/chat"
MAIN = "gemma4:31b-cloud"
SECOND = "qwen2.5-coder:1.5b"
P_MAIN = (0.27 / 1e6, 1.10 / 1e6)
P_SECOND = (0.05 / 1e6, 0.20 / 1e6)
REPS = 5
ABL_REPS = 3


def ollama(model, prompt, max_tokens=160):
    body = json.dumps({"model": model, "stream": False,
                       "options": {"temperature": 0, "num_predict": max_tokens},
                       "messages": [{"role": "user", "content": prompt}]}).encode()
    req = urllib.request.Request(OLLAMA, data=body, headers={"Content-Type": "application/json"})
    t0 = time.time()
    with urllib.request.urlopen(req, timeout=180) as r:
        v = json.loads(r.read())
    return {"text": v.get("message", {}).get("content", ""),
            "input": v.get("prompt_eval_count", 0), "output": v.get("eval_count", 0),
            "latency": int((time.time() - t0) * 1000)}


def run(env, *args):
    p = subprocess.run([HLI, *args], env=env, capture_output=True, text=True)
    if p.returncode != 0:
        raise RuntimeError(f"{args}: {p.stderr or p.stdout}")
    return json.loads(p.stdout) if p.stdout.strip().startswith(("{", "[")) else p.stdout


def load_log():
    lines = [f"2026-11-07T10:{i%60:02d}:00Z INFO request id={i} path=/api/x status=200" for i in range(1500)]
    lines.insert(1500, "2026-11-07T11:00:00Z ERROR db timeout after 30s pool=8")
    lines += [f"2026-11-07T11:{i%60:02d}:00Z INFO done id={i}" for i in range(400)]
    return "\n".join(lines)


def load_repo():
    files = {}
    for i in range(15):
        name = f"mod_{i}.rs"
        body = f"// {name}\npub fn helper_{i}() -> i32 {{ {i} }}\n"
        if i == 9:
            body += "pub fn compute_total(xs: &[i32]) -> i32 { xs.iter().sum() }\n"
        files[name] = body
    return files


def main():
    base = tempfile.mkdtemp(prefix="hli-bench3-")
    env = dict(os.environ, HLI_HOME=os.path.join(base, "home"))
    os.makedirs(env["HLI_HOME"], exist_ok=True)
    open(os.path.join(env["HLI_HOME"], "prices.yaml"), "w").write(
        f"provider: ollama\nmodel: gemma4:31b\ncurrency: USD\ninput_per_million: 0.27\n"
        f"output_per_million: 1.10\neffective_from: 2026-11-07\nsource: user-configured\n"
        f"provider: ollama\nmodel: qwen2.5-coder:1.5b\ncurrency: USD\ninput_per_million: 0.05\n"
        f"output_per_million: 0.20\neffective_from: 2026-11-07\nsource: user-configured\n")
    proc = subprocess.Popen([DAEMON], env=env, stdin=subprocess.DEVNULL,
                            stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    for _ in range(100):
        time.sleep(0.05)
        if subprocess.run([HLI, "daemon", "status"], env=env, capture_output=True).returncode == 0:
            break
    rows, abl = [], []
    try:
        ws = run(env, "workspace", "create", "bench3", "--root", base)["id"]
        th = run(env, "thread", "create", "bench3", "--workspace", ws)["id"]
        br = run(env, "branch", "create", "main", "--thread", th)["id"]

        # Carga 1: log grande con slice alrededor del error
        log = load_log()
        log_path = os.path.join(base, "big.log")
        open(log_path, "w", encoding="utf-8").write(log)
        log_aid = run(env, "artifact", "put", log_path, "--workspace", ws)["id"]
        run(env, "artifact", "slice", log_aid, "--workspace", ws, "--kind", "line_range",
            "--start", "1495", "--end", "1510", "--reason", "error window")
        run(env, "memory", "commit", "--workspace", ws, "--type", "fact",
            "--content", "El error grave es db timeout after 30s pool=8.")
        log_task = {"id": "log_big", "prompt": "¿Qué error grave aparece en el log?",
                    "check": lambda t: "db timeout" in t.lower(), "artifact": log_aid,
                    "ctx": log}

        # Carga 2: repo de 15 archivos
        repo = load_repo()
        repo_text = "\n".join(f"### {k}\n{v}" for k, v in repo.items())
        repo_path = os.path.join(base, "repo.txt")
        open(repo_path, "w", encoding="utf-8").write(repo_text)
        repo_aid = run(env, "artifact", "put", repo_path, "--workspace", ws)["id"]
        run(env, "artifact", "slice", repo_aid, "--workspace", ws, "--kind", "full_text_if_small",
            "--reason", "repo snapshot")
        run(env, "memory", "commit", "--workspace", ws, "--type", "fact",
            "--content", "compute_total está definida en mod_9.rs.")
        repo_task = {"id": "repo_15", "prompt": "¿En qué archivo se define compute_total?",
                     "check": lambda t: "mod_9" in t, "artifact": repo_aid, "ctx": repo_text}

        # Carga 3: thread con 100 eventos
        for i in range(100):
            payload = '{"n":%d}' % i
            if i == 60:
                payload = '{"decision":"SQLite es la fuente de verdad"}'
            run(env, "event", "append", "--type", "note", "--payload", payload,
                "--workspace", ws, "--thread", th)
        run(env, "memory", "commit", "--workspace", ws, "--type", "decision",
            "--content", "Se decidió que SQLite es la fuente de verdad.")
        thread_task = {"id": "thread_100", "prompt": "¿Qué decisión se tomó sobre la base de datos?",
                       "check": lambda t: "sqlite" in t.lower(), "artifact": None, "ctx": None}

        for t in [log_task, repo_task, thread_task]:
            for rep in range(REPS):
                # baseline
                if t["ctx"]:
                    r = ollama(MAIN, f"Contexto:\n{t['ctx']}\n\nTarea: {t['prompt']}")
                else:
                    evs = run(env, "event", "list", "--workspace", ws)
                    ctx = "\n".join(json.dumps(e["payload"], ensure_ascii=False) for e in evs[:100])
                    r = ollama(MAIN, f"Historial:\n{ctx}\n\nTarea: {t['prompt']}")
                ok = t["check"](r["text"])
                rows.append({"task": t["id"], "mode": "baseline", "rep": rep, "input": r["input"],
                             "output": r["output"], "latency": r["latency"], "ok": ok,
                             "cost": r["input"] * P_MAIN[0] + r["output"] * P_MAIN[1]})
                # hli v2
                args = ["run", "start", "--workspace", ws, "--thread", th, "--branch", br,
                        "--prompt", t["prompt"], "--provider", "ollama", "--model", MAIN,
                        "--max-tokens", "160"]
                if t["artifact"]:
                    args += ["--artifact", t["artifact"]]
                t0 = time.time()
                rv = run(env, *args)
                got = run(env, "run", "get", rv["id"])
                u = got["run"]["receipt"]["usage"]
                ok = t["check"](got["run"]["result"].get("text", ""))
                rows.append({"task": t["id"], "mode": "hli_v2", "rep": rep,
                             "input": u["input_tokens"], "output": u["output_tokens"],
                             "latency": int((time.time() - t0) * 1000), "ok": ok,
                             "cost": u["input_tokens"] * P_MAIN[0] + u["output_tokens"] * P_MAIN[1]})
                # routed: secundario extrae, principal integra
                if t["ctx"]:
                    ex = ollama(SECOND, f"Extrae solo la respuesta a: {t['prompt']}\nDe:\n{t['ctx']}", 80)
                else:
                    ex = ollama(SECOND, f"Extrae la decisión sobre la base de datos del historial.", 80)
                fin = ollama(MAIN, f"Datos: {ex['text']}\nResponde: {t['prompt']}", 120)
                ok = t["check"](fin["text"])
                cost = (ex["input"] * P_SECOND[0] + ex["output"] * P_SECOND[1]
                        + fin["input"] * P_MAIN[0] + fin["output"] * P_MAIN[1])
                rows.append({"task": t["id"], "mode": "routed", "rep": rep,
                             "input": ex["input"] + fin["input"], "output": ex["output"] + fin["output"],
                             "latency": ex["latency"] + fin["latency"], "ok": ok, "cost": cost})

        # Ablaciones sobre log_big
        variants = {
            "full": [],
            "no_memories": ["--no-memories"],
            "no_events": ["--no-events"],
            "no_slices": ["--no-slices"],
            "no_policy": ["--no-policy"],
        }
        for name, flags in variants.items():
            for rep in range(ABL_REPS):
                args = ["run", "start", "--workspace", ws, "--thread", th, "--branch", br,
                        "--prompt", log_task["prompt"], "--provider", "ollama", "--model", MAIN,
                        "--artifact", log_task["artifact"], "--max-tokens", "160"] + flags
                rv = run(env, *args)
                got = run(env, "run", "get", rv["id"])
                u = got["run"]["receipt"]["usage"]
                ok = log_task["check"](got["run"]["result"].get("text", ""))
                abl.append({"variant": name, "rep": rep, "input": u["input_tokens"],
                            "output": u["output_tokens"], "ok": ok,
                            "cost": u["input_tokens"] * P_MAIN[0] + u["output_tokens"] * P_MAIN[1]})
    finally:
        subprocess.run([HLI, "daemon", "stop"], env=env, capture_output=True)
        proc.wait(timeout=5)
        shutil.rmtree(base, ignore_errors=True)

    def stats(vals):
        vals = sorted(vals)
        if not vals:
            return (0, 0, 0, 0, 0)
        p = lambda q: vals[min(len(vals) - 1, int(round(q * (len(vals) - 1))))]
        return (p(0.5), p(0.9), vals[0], vals[-1], statistics.pstdev(vals))

    lines = ["# HLI — Benchmark v3", "",
             f"Principal `{MAIN}`, secundario `{SECOND}` (local). {REPS} repeticiones. "
             "Precios de referencia: principal $0.27/$1.10, secundario $0.05/$0.20 por millón.", "",
             "| Tarea | Modo | Input p50 | p90 | Latencia p50 | Calidad | Coste p50 |",
             "|---|---|---|---|---|---|---|"]
    for t in ["log_big", "repo_15", "thread_100"]:
        for mode in ["baseline", "hli_v2", "routed"]:
            sel = [r for r in rows if r["task"] == t and r["mode"] == mode]
            if not sel:
                continue
            i = stats([r["input"] for r in sel])
            l = stats([r["latency"] for r in sel])
            c = stats([r["cost"] for r in sel])
            q = sum(1 for r in sel if r["ok"])
            lines.append(f"| {t} | {mode} | {i[0]:.0f} | {i[1]:.0f} | {l[0]:.0f} | {q}/{len(sel)} | ${c[0]:.6f} |")
    lines.append("")
    for mode in ["baseline", "hli_v2", "routed"]:
        sel = [r for r in rows if r["mode"] == mode]
        i = stats([r["input"] for r in sel]); c = stats([r["cost"] for r in sel])
        lines.append(f"- **{mode}**: input p50 {i[0]:.0f} (p90 {i[1]:.0f}, min {i[2]}, max {i[3]}, sd {i[4]:.0f}); "
                     f"coste p50 ${c[0]:.6f}; calidad {sum(1 for r in sel if r['ok'])}/{len(sel)}")
    open(os.path.join(ROOT, "docs", "BENCHMARK-REPORT-v3.md"), "w", encoding="utf-8").write("\n".join(lines) + "\n")

    al = ["# HLI — Ablaciones del Context Engine", "",
          f"Carga: log de {len(load_log())} caracteres. {ABL_REPS} repeticiones. "
          "Se quita un componente cada vez.", "",
          "| Variante | Input p50 | Calidad | Coste p50 |", "|---|---|---|---|"]
    for name in ["full", "no_memories", "no_events", "no_slices", "no_policy"]:
        sel = [r for r in abl if r["variant"] == name]
        i = stats([r["input"] for r in sel]); c = stats([r["cost"] for r in sel])
        q = sum(1 for r in sel if r["ok"])
        al.append(f"| {name} | {i[0]:.0f} | {q}/{len(sel)} | ${c[0]:.6f} |")
    open(os.path.join(ROOT, "docs", "CONTEXT-ABLATION.md"), "w", encoding="utf-8").write("\n".join(al) + "\n")
    print(json.dumps({"ok": True, "rows": len(rows), "ablations": len(abl),
                      "reports": ["docs/BENCHMARK-REPORT-v3.md", "docs/CONTEXT-ABLATION.md"]}))


if __name__ == "__main__":
    main()
