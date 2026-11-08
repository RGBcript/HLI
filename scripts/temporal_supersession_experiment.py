#!/usr/bin/env python3
"""Experimento de supersesión temporal: A sin supersesión, B con supersesión explícita."""
import json
import os
import statistics
import subprocess
import tempfile
import time

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
HLI = os.path.join(ROOT, "target", "debug", "hli.exe")
DAEMON = os.path.join(ROOT, "target", "debug", "hli-daemon.exe")
MODEL = "gemma4:31b-cloud"
PRICE = (0.27 / 1e6, 1.10 / 1e6)

PAIRS = []
for i in range(20):
    old = f"El límite del recurso {i} es {100 + i}."
    new = f"El límite del recurso {i} es {200 + i}."
    PAIRS.append({"old": old, "new": new, "query": f"¿Cuál es el límite del recurso {i}?",
                  "expected": str(200 + i)})
SINGLES = []
for i in range(10):
    port = 9000 + i
    SINGLES.append({"content": f"El puerto del servicio {i} es {port}.",
                    "query": f"¿Qué puerto usa el servicio {i}?", "expected": str(port)})
DISTRACTORS = [
    "El límite del recurso auxiliar A es 999.", "El límite del recurso auxiliar B es 888.",
    "El puerto del servicio auxiliar A es 7001.", "El puerto del servicio auxiliar B es 7002.",
    "El recurso A tiene un límite distinto en producción.", "El recurso B usa otro puerto en staging.",
    "El límite del recurso C se cambió en 2024.", "El puerto del servicio C se movió a 7100.",
    "El recurso D no tiene límite definido.", "El servicio D no expone puerto.",
]


def run(env, *args):
    p = subprocess.run([HLI, *args], env=env, capture_output=True, text=True)
    if p.returncode != 0:
        raise RuntimeError(f"{args}: {p.stderr or p.stdout}")
    return json.loads(p.stdout) if p.stdout.strip().startswith(("{", "[")) else p.stdout


def start(env):
    p = subprocess.Popen([DAEMON], env=env, stdin=subprocess.DEVNULL,
                         stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    for _ in range(100):
        time.sleep(0.05)
        if subprocess.run([HLI, "daemon", "status"], env=env, capture_output=True).returncode == 0:
            return p
    raise RuntimeError("daemon no arrancó")


def main():
    base = tempfile.mkdtemp(prefix="hli-temporal-")
    env = dict(os.environ, HLI_HOME=os.path.join(base, "home"))
    os.makedirs(env["HLI_HOME"], exist_ok=True)
    open(os.path.join(env["HLI_HOME"], "prices.yaml"), "w").write(
        "provider: ollama\nmodel: gemma4:31b\ncurrency: USD\ninput_per_million: 0.27\n"
        "output_per_million: 1.10\neffective_from: 2026-11-07\nsource: user-configured\n")
    d = start(env)
    rows = []
    try:
        for arm in ["A", "B"]:
            ws = run(env, "workspace", "create", f"temporal_{arm}", "--root", base)["id"]
            th = run(env, "thread", "create", "t", "--workspace", ws)["id"]
            br = run(env, "branch", "create", "main", "--thread", th)["id"]
            ids = []
            for p in PAIRS:
                old = run(env, "memory", "commit", "--workspace", ws, "--type", "fact",
                          "--content", p["old"], "--authority", "legacy")["id"]
                if arm == "B":
                    new = run(env, "memory", "commit", "--workspace", ws, "--type", "fact",
                              "--content", p["new"], "--authority", "current",
                              "--supersedes", old)["id"]
                else:
                    new = run(env, "memory", "commit", "--workspace", ws, "--type", "fact",
                              "--content", p["new"], "--authority", "current")["id"]
                ids.append((old, new))
            single_ids = []
            for s in SINGLES:
                single_ids.append(run(env, "memory", "commit", "--workspace", ws, "--type", "fact",
                                      "--content", s["content"])["id"])
            for content in DISTRACTORS:
                run(env, "memory", "commit", "--workspace", ws, "--type", "fact", "--content", content)

            for i, p in enumerate(PAIRS):
                old_id, new_id = ids[i]
                found = run(env, "memory", "search", "--workspace", ws, "--query", p["query"],
                            "--limit", "10")
                pos = {m["id"]: j for j, m in enumerate(found)}
                stale = 1 if old_id in pos else 0
                cur = 1 if new_id in pos else 0
                rank = 1 if (cur and (not stale or pos[new_id] < pos[old_id])) else 0
                t0 = time.time()
                rv = run(env, "run", "start", "--workspace", ws, "--thread", th, "--branch", br,
                         "--prompt", p["query"], "--provider", "ollama", "--model", MODEL,
                         "--max-tokens", "32", "--top-k", "5")
                got = run(env, "run", "get", rv["id"])
                u = got["run"]["receipt"]["usage"]
                text = got["run"]["result"].get("text", "")
                rows.append({"group": "pair", "arm": arm, "i": i, "stale_retrieved": stale,
                             "current_recall": cur, "ranking_ok": rank,
                             "quality": 1 if p["expected"] in text else 0,
                             "tokens_in": u["input_tokens"], "latency_ms": int((time.time() - t0) * 1000),
                             "answer": text[:160]})
            for i, s in enumerate(SINGLES):
                found = run(env, "memory", "search", "--workspace", ws, "--query", s["query"],
                            "--limit", "10")
                cur = 1 if any(m["id"] == single_ids[i] for m in found) else 0
                rv = run(env, "run", "start", "--workspace", ws, "--thread", th, "--branch", br,
                         "--prompt", s["query"], "--provider", "ollama", "--model", MODEL,
                         "--max-tokens", "32", "--top-k", "5")
                got = run(env, "run", "get", rv["id"])
                text = got["run"]["result"].get("text", "")
                rows.append({"group": "single", "arm": arm, "i": i, "stale_retrieved": 0,
                             "current_recall": cur, "ranking_ok": cur,
                             "quality": 1 if s["expected"] in text else 0,
                             "tokens_in": got["run"]["receipt"]["usage"]["input_tokens"],
                             "latency_ms": 0, "answer": text[:160]})
            # estado interno de las old en B
            if arm == "B":
                for old_id, _ in ids:
                    m = run(env, "memory", "get", old_id)
                    rows.append({"group": "state", "arm": arm, "i": 0,
                                 "status": m["status"], "superseded_at": m["superseded_at"],
                                 "stale_retrieved": 0, "current_recall": 0,
                                 "ranking_ok": 0, "quality": 0, "tokens_in": 0, "latency_ms": 0,
                                 "answer": ""})
    finally:
        subprocess.run([HLI, "daemon", "stop"], env=env, capture_output=True)
        try:
            d.wait(timeout=5)
        except Exception:
            d.kill()

    os.makedirs(os.path.join(ROOT, "docs", "research"), exist_ok=True)
    open(os.path.join(ROOT, "docs", "research", "TEMPORAL-SUPERSESSION-RAW.jsonl"), "w",
         encoding="utf-8").write("\n".join(json.dumps(r, ensure_ascii=False) for r in rows) + "\n")

    def agg(arm, group):
        sel = [r for r in rows if r["arm"] == arm and r["group"] == group]
        return {
            "n": len(sel),
            "stale_rate": statistics.mean(r["stale_retrieved"] for r in sel) if sel else 0,
            "current_recall": statistics.mean(r["current_recall"] for r in sel) if sel else 0,
            "ranking": statistics.mean(r["ranking_ok"] for r in sel) if sel else 0,
            "quality": sum(r["quality"] for r in sel),
            "tokens": statistics.mean(r["tokens_in"] for r in sel) if sel else 0,
        }
    summary = {
        "A_pairs": agg("A", "pair"), "B_pairs": agg("B", "pair"),
        "A_singles": agg("A", "single"), "B_singles": agg("B", "single"),
        "B_state_superseded": sum(1 for r in rows if r["group"] == "state" and r["status"] == "superseded"),
        "B_state_total": sum(1 for r in rows if r["group"] == "state"),
    }
    open(os.path.join(ROOT, "docs", "research", "temporal-summary.json"), "w", encoding="utf-8").write(
        json.dumps(summary, indent=2))
    print(json.dumps(summary))


if __name__ == "__main__":
    main()
