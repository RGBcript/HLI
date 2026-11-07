#!/usr/bin/env python3
"""Experimento Adaptive-k sobre SQLite FTS5.

A: top-5 fijo. B: k variable por gap normalizado sobre relevancia positiva.
Calibración y evaluación separadas y congeladas. Workspace limpio por brazo/rep.
"""
import hashlib
import json
import os
import random
import shutil
import statistics
import subprocess
import tempfile
import time

ROOT = r"C:\Users\GUERRA\Documents\TEC\HLI"
HLI = os.path.join(ROOT, "target", "debug", "hli.exe")
DAEMON = os.path.join(ROOT, "target", "debug", "hli-daemon.exe")
MAIN = "gemma4:31b-cloud"
PRICE = (0.27 / 1e6, 1.10 / 1e6)
REPS = 3

MEMORIES = [
    ("SQLite es la fuente de verdad del proyecto.", "fuente de verdad"),
    ("PostgreSQL fue la fuente de verdad antes de la migración.", None),
    ("La API usa JWT con expiración de 15 minutos.", "JWT"),
    ("La API usaba sesiones de servidor en la versión 0.1.", None),
    ("El build se ejecuta con cargo build --workspace.", "build"),
    ("El build se ejecutaba con make antes de migrar.", None),
    ("Los tests de integración viven en crates/hli-cli/tests.", "tests de integración"),
    ("Los tests unitarios viven en src junto al código.", None),
    ("El límite de la cola es 10 000 mutaciones.", "cola"),
    ("La cola tenía 1 000 mutaciones en el diseño inicial.", None),
    ("La versión de Rust es 1.99.", "Rust"),
    ("Rust 1.80 era el mínimo anterior.", None),
    ("El puerto del daemon es efímero y se publica en daemon.json.", "daemon.json"),
    ("El puerto era fijo 8080 en el prototipo.", None),
    ("Los artefactos se guardan con SHA-256.", "SHA-256"),
    ("Los artefactos se guardaban en SQLite antes del hash.", None),
    ("El protocolo de módulos usa JSON-RPC 2.0 sobre stdio.", None),
    ("Los módulos no pueden abrir hli.db directamente.", None),
    ("El writer actor es el único que escribe en SQLite.", None),
    ("El event log es append-only y verificable.", None),
    ("El ContextManifest incluye user_input y memorias.", None),
    ("El payload_hash cubre el payload canónico completo.", None),
    ("Los artifact slices usan line_range o byte_range.", None),
    ("La política de contexto actual usa top-5 fijo.", None),
    ("El proveedor simulado no hace red ni coste.", None),
    ("Ollama expone /api/chat en el puerto 11434.", None),
    ("El proveedor OpenAI-compatible usa /chat/completions.", None),
    ("Los secretos se leen por handle, nunca de SQLite.", None),
    ("El receipt guarda usage y coste calculado.", None),
    ("El modo routed con qwen1.5b dio 0/15.", None),
    ("El benchmark v3 usó un log de 111 534 caracteres.", None),
    ("La memoria tiene tipos fact, decision, constraint y summary.", None),
    ("El estado superseded excluye la memoria de la búsqueda.", None),
    ("La vigencia filtra valid_from y valid_until.", None),
    ("El manifiesto guarda visible_event_ids como evidencia.", None),
    ("La rama hija corta en fork_event.", None),
    ("El writer rechaza duplicados por idempotency_key.", None),
    ("El artefacto se direcciona por hash de contenido.", None),
    ("El daemon publica pid y puerto en daemon.json.", None),
    ("El CLI nunca abre SQLite directamente.", None),
]

QUERIES_CAL = [
    {"id": "cal_fuente", "prompt": "¿Cuál es la fuente de verdad?", "expected": "sqlite", "match": "fuente de verdad"},
    {"id": "cal_jwt", "prompt": "¿Qué autenticación usa la API?", "expected": "jwt", "match": "JWT"},
    {"id": "cal_build", "prompt": "¿Cómo se ejecuta el build?", "expected": "cargo build", "match": "build"},
    {"id": "cal_tests", "prompt": "¿Dónde están los tests de integración?", "expected": "crates/hli-cli/tests", "match": "tests de integración"},
    {"id": "cal_cola", "prompt": "¿Cuál es el límite de la cola?", "expected": "10 000", "match": "cola"},
    {"id": "cal_rust", "prompt": "¿Qué versión de Rust se usa?", "expected": "1.99", "match": "Rust"},
    {"id": "cal_puerto", "prompt": "¿Dónde se publica el puerto del daemon?", "expected": "daemon.json", "match": "daemon.json"},
    {"id": "cal_hash", "prompt": "¿Qué hash usan los artefactos?", "expected": "sha-256", "match": "SHA-256"},
]

QUERIES_EVAL = [
    {"id": "eval_fuente", "prompt": "¿Qué base de datos es la fuente de verdad del proyecto?", "expected": "sqlite", "match": "fuente de verdad"},
    {"id": "eval_jwt", "prompt": "¿Cuánto dura el token JWT?", "expected": "15", "match": "JWT"},
    {"id": "eval_build", "prompt": "¿Qué comando compila todo el workspace?", "expected": "cargo build", "match": "build"},
    {"id": "eval_tests", "prompt": "¿En qué carpeta están las pruebas de integración?", "expected": "crates/hli-cli/tests", "match": "tests de integración"},
    {"id": "eval_cola", "prompt": "¿Cuántas mutaciones caben en la cola?", "expected": "10 000", "match": "cola"},
    {"id": "eval_rust", "prompt": "¿Qué versión de Rust usa el proyecto?", "expected": "1.99", "match": "Rust"},
    {"id": "eval_puerto", "prompt": "¿Dónde se anuncia el puerto del daemon?", "expected": "daemon.json", "match": "daemon.json"},
    {"id": "eval_hash", "prompt": "¿Qué algoritmo de hash usan los artefactos?", "expected": "sha-256", "match": "SHA-256"},
]


def run(env, *args):
    p = subprocess.run([HLI, *args], env=env, capture_output=True, text=True)
    if p.returncode != 0:
        raise RuntimeError(f"{args}: {p.stderr or p.stdout}")
    return json.loads(p.stdout) if p.stdout.strip().startswith(("{", "[")) else p.stdout


def choose_k(rels, threshold):
    """rels: relevancias positivas, mayor = mejor. Gap normalizado sobre la mejor."""
    if len(rels) < 2:
        return 1 if rels else 5
    n = min(9, len(rels) - 1)
    gaps = [(rels[i] - rels[i + 1]) / max(rels[i], 1e-9) for i in range(n)]
    best = max(range(len(gaps)), key=lambda i: gaps[i])
    if gaps[best] >= threshold:
        return best + 1
    return 5


def start(env):
    p = subprocess.Popen([DAEMON], env=env, stdin=subprocess.DEVNULL,
                         stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    for _ in range(100):
        time.sleep(0.05)
        if subprocess.run([HLI, "daemon", "status"], env=env, capture_output=True).returncode == 0:
            return p
    raise RuntimeError("daemon no arrancó")


def main():
    base = tempfile.mkdtemp(prefix="hli-adapt-")
    # Congelar datasets antes de ejecutar
    dataset = {"memories": MEMORIES, "calibration": QUERIES_CAL, "evaluation": QUERIES_EVAL}
    raw = json.dumps(dataset, ensure_ascii=False, sort_keys=True).encode()
    ds_hash = "sha256:" + hashlib.sha256(raw).hexdigest()
    os.makedirs(os.path.join(ROOT, "docs", "research"), exist_ok=True)
    open(os.path.join(ROOT, "docs", "research", "adaptive-k-dataset.json"), "wb").write(raw)

    env = dict(os.environ, HLI_HOME=os.path.join(base, "home"))
    os.makedirs(env["HLI_HOME"], exist_ok=True)
    open(os.path.join(env["HLI_HOME"], "prices.yaml"), "w").write(
        "provider: ollama\nmodel: gemma4:31b\ncurrency: USD\ninput_per_million: 0.27\n"
        "output_per_million: 1.10\neffective_from: 2026-11-07\nsource: user-configured\n")
    d = start(env)
    rows = []
    try:
        # Workspace de calibración
        ws = run(env, "workspace", "create", "cal", "--root", base)["id"]
        ids = {}
        for content, match in MEMORIES:
            mid = run(env, "memory", "commit", "--workspace", ws, "--type", "fact",
                      "--content", content)["id"]
            ids[content] = mid
            if match:
                ids[match] = mid
        cal_scores = {}
        for q in QUERIES_CAL:
            found = run(env, "memory", "search", "--workspace", ws, "--query", q["prompt"],
                        "--limit", "50")
            cal_scores[q["id"]] = found
        # Calibrar umbral: mejor recall, luego menor k medio
        best = None
        for th in [0.05, 0.08, 0.10, 0.12, 0.15, 0.20, 0.25, 0.30]:
            recalls, ks = [], []
            for q in QUERIES_CAL:
                found = cal_scores[q["id"]]
                rels = [m["score"] for m in found]
                k = choose_k(rels, th)
                ks.append(k)
                rid = ids[q["match"]]
                recalls.append(1 if any(m["id"] == rid for m in found[:k]) else 0)
            score = (sum(recalls), -statistics.mean(ks))
            if best is None or score > best[0]:
                best = (score, th, statistics.mean(ks), sum(recalls))
        threshold = best[1]
        cal_summary = {"threshold": threshold, "mean_k": best[2], "recall": best[3] / len(QUERIES_CAL)}

        # Evaluación: 2 brazos × 3 reps × 8 consultas
        for rep in range(REPS):
            for q in QUERIES_EVAL:
                order = ["A", "B"]
                random.shuffle(order)
                for arm in order:
                    ws2 = run(env, "workspace", "create", f"eval_{arm}_{rep}", "--root", base)["id"]
                    ids2 = {}
                    for content, match in MEMORIES:
                        mid = run(env, "memory", "commit", "--workspace", ws2, "--type", "fact",
                                  "--content", content)["id"]
                        ids2[content] = mid
                        if match:
                            ids2[match] = mid
                    found = run(env, "memory", "search", "--workspace", ws2, "--query", q["prompt"],
                                "--limit", "50")
                    rels = [m["score"] for m in found]
                    k = 5 if arm == "A" else choose_k(rels, threshold)
                    rid = ids2[q["match"]]
                    top = found[:k]
                    recall = 1 if any(m["id"] == rid for m in top) else 0
                    precision = recall / k if k else 0.0
                    th = run(env, "thread", "create", "t", "--workspace", ws2)["id"]
                    br = run(env, "branch", "create", "main", "--thread", th)["id"]
                    t0 = time.time()
                    rv = run(env, "run", "start", "--workspace", ws2, "--thread", th,
                             "--branch", br, "--prompt", q["prompt"], "--provider", "ollama",
                             "--model", MAIN, "--max-tokens", "80", "--top-k", str(k))
                    got = run(env, "run", "get", rv["id"])
                    u = got["run"]["receipt"]["usage"]
                    text = got["run"]["result"].get("text", "")
                    rows.append({
                        "dataset": "eval", "query": q["id"], "arm": arm, "rep": rep,
                        "k": k, "bm25": rels[:10], "selected": [m["id"] for m in top],
                        "expected_id": rid, "recall": recall, "precision": precision,
                        "tokens_in": u["input_tokens"], "tokens_out": u["output_tokens"],
                        "cost": u["input_tokens"] * PRICE[0] + u["output_tokens"] * PRICE[1],
                        "latency_ms": int((time.time() - t0) * 1000),
                        "answer_ok": q["expected"] in text.lower(),
                        "answer": text[:200],
                    })
    finally:
        subprocess.run([HLI, "daemon", "stop"], env=env, capture_output=True)
        try:
            d.wait(timeout=5)
        except Exception:
            d.kill()
        shutil.rmtree(base, ignore_errors=True)

    open(os.path.join(ROOT, "docs", "research", "ADAPTIVE-K-RAW.jsonl"), "w", encoding="utf-8").write(
        "\n".join(json.dumps(r, ensure_ascii=False) for r in rows) + "\n")

    def agg(arm):
        sel = [r for r in rows if r["arm"] == arm]
        return {
            "n": len(sel),
            "k_mean": statistics.mean(r["k"] for r in sel),
            "recall": statistics.mean(r["recall"] for r in sel),
            "precision": statistics.mean(r["precision"] for r in sel),
            "tokens_in": statistics.mean(r["tokens_in"] for r in sel),
            "cost": sum(r["cost"] for r in sel),
            "latency": statistics.mean(r["latency_ms"] for r in sel),
            "quality": sum(1 for r in sel if r["answer_ok"]),
        }
    A, B = agg("A"), agg("B")
    print(json.dumps({"dataset_hash": ds_hash, "calibration": cal_summary, "A": A, "B": B}))
    open(os.path.join(ROOT, "docs", "research", "adaptive-k-summary.json"), "w", encoding="utf-8").write(
        json.dumps({"dataset_hash": ds_hash, "calibration": cal_summary, "A": A, "B": B}, indent=2))


if __name__ == "__main__":
    main()
