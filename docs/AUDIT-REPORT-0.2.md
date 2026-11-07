# HLI — Informe de fase 0.2

**Fase:** HLI 0.2 — contexto y ejecución
**Estado:** cerrada y verificada
**Commit:** `1a484dd`
**Especificación de referencia:** `HLI_Specification_Draft_v0.14.docx` (§6, §15.5, §15.7, §16.3)
**Audiencia:** Copilot (auditor externo)
**Fecha:** 2026-11-07

---

## 1. Resumen ejecutivo

0.2 añade ejecución real sobre el sustrato de 0.1: un `Run` queda fijado a una revisión de rama, se construye y persiste un `ContextManifest` **antes** de emitir `model.requested`, un proveedor determinista produce la respuesta, el Tool Runtime ejecuta operaciones con permisos `allow/ask/deny` y se emite un `Reproducibility Receipt` asociado al run.

**Demostrado con pruebas:** el orden manifiesto→request, el receipt, la recuperación exacta tras reiniciar el daemon, el confinamiento del Tool Runtime al root del workspace y los tres niveles de permiso.

**No implementado en 0.2:** memoria, compactación, retrieval, streaming, proveedores reales, sandbox de sistema operativo y MCP. El proveedor es simulado y `shell.execute` está denegado por política: no se finge una capacidad que no existe.

---

## 2. Reproducción

```bash
cd C:\Users\GUERRA\Documents\TEC\HLI
cargo build --workspace
cargo test
```

Salida real: **25 pruebas en verde** (10 core, 2 store, 5 agent, 8 e2e).

Ciclo manual del gate:

```bash
export HLI_HOME=C:/tmp/hli-02
target/debug/hli daemon start
target/debug/hli workspace create demo --root C:/tmp/hli-root
target/debug/hli thread create research --workspace <ws>
target/debug/hli branch create main --thread <thr>
target/debug/hli run start --workspace <ws> --thread <thr> --branch <br> --prompt "hola"
target/debug/hli run get <run>          # run + manifest
target/debug/hli event list --workspace <ws>
target/debug/hli daemon stop
target/debug/hli daemon start
target/debug/hli run get <run>          # mismo estado y mismo content_hash
```

---

## 3. Flujo de un run

```
CLI ──run.start──▶ daemon
                     │ create_run()            → run.queued
                     │ build_manifest()        (eventos ≤ branch_revision)
                     │ save_manifest()         → context.manifest.created
                     │ run_event()             → model.requested
                     │ SimProvider.complete()
                     │ run_event()             → model.responded
                     │ [tool.requested → tool.completed | tool.denied]
                     │ finish_run()            → run.completed | failed | waiting_approval
                     ▼
                  receipt persistido en runs.receipt_json
```

Invariante verificada: la secuencia de `context.manifest.created` es menor que la de `model.requested` en el mismo workspace.

---

## 4. Contratos nuevos

### 4.1 Run

```json
{
  "id": "run_...",
  "workspace_id": "ws_...",
  "thread_id": "thr_...",
  "branch_id": "br_...",
  "branch_revision": 3,
  "state": "completed",
  "prompt": "hola",
  "started_at": "2026-11-07T08:40:00.000Z",
  "ended_at": "2026-11-07T08:40:00.010Z",
  "result": {"text": "sim:1a2b3c4d", "manifest_id": "ctx_..."},
  "receipt": { "...": "..." }
}
```

Estados usados: `queued`, `completed`, `failed`, `waiting_approval`.

### 4.2 ContextManifest

```json
{
  "manifest_id": "ctx_...",
  "policy_version": "context/v1",
  "workspace_id": "ws_...",
  "run_id": "run_...",
  "branch_revision": 3,
  "budget_tokens": 8192,
  "instructions": [],
  "memories": [],
  "recent_events": [
    {"event_id": "evt_...", "sequence": 2, "type": "thread.created", "snippet": "..."},
    {"event_id": "evt_...", "sequence": 3, "type": "branch.created", "snippet": "..."}
  ],
  "summaries": [],
  "artifact_slices": [],
  "excluded": [],
  "estimated_tokens": 42,
  "content_hash": "sha256:...",
  "created_at": "2026-11-07T08:40:00.005Z"
}
```

- Se sella con `content_hash` sobre el sobre sin ese campo.
- Se guarda completo en `context_manifests.body_json` para poder reconstruirlo.
- Los eventos con `sequence > branch_revision` quedan fuera por diseño.
- Lo que no cabe en el presupuesto se registra en `excluded` con `reason: budget`.

### 4.3 Receipt

```json
{
  "provider": "sim",
  "model": "sim-1",
  "adapter_version": "0.2.0",
  "context_manifest": "ctx_...",
  "tool_schemas": ["filesystem.read", "filesystem.write"],
  "effective_permissions": {
    "filesystem.read": "allow",
    "filesystem.write": "ask",
    "shell.execute": "deny"
  },
  "nonsecret_environment": {"os": "windows", "arch": "x86_64"},
  "secret_handles": [],
  "input_artifacts": [],
  "module_versions": [],
  "sampling": {"temperature": 0.0, "seed": 0},
  "result_artifacts": [],
  "sandbox": {"mode": "none", "note": "0.2 no implementa sandbox; shell denegado"},
  "terminal_event": null
}
```

`terminal_event` queda nulo porque `run.completed` se emite después de construir el receipt. Es deuda declarada, no un olvido silencioso.

### 4.4 ToolOutcome

```json
{
  "tool": "filesystem.read",
  "status": "success",
  "exit_code": 0,
  "stdout": "contenido",
  "changed_files": [],
  "error": null
}
```

---

## 5. Esquema añadido

```sql
CREATE TABLE runs (
  id, workspace_id, thread_id, branch_id, branch_revision,
  state, prompt, started_at, ended_at, result_json, receipt_json
);

CREATE TABLE context_manifests (
  id, workspace_id, run_id, branch_revision, budget_tokens,
  estimated_tokens, content_hash, body_json, created_at
);
```

Índices: `runs(workspace_id, started_at)`, `context_manifests(run_id)`.

---

## 6. Gate de 0.2: evidencia

| Prueba | Qué demuestra |
|---|---|
| `run_manifest_receipt_and_restart` | run fijado a revisión 3; manifiesto sellado; receipt con `context_manifest`; orden manifest(5) < requested(6); mismo `content_hash` tras reiniciar; `verify` ok |
| `tool_read_is_allowed` | `filesystem.read` se ejecuta sin aprobación y devuelve el contenido |
| `tool_write_requires_approval` | `filesystem.write` deja el run en `waiting_approval`, no escribe; `run.approve` lo completa y escribe |
| `tool_shell_is_denied` | `shell.execute` termina el run en `failed` con error de política |
| Unitarias del agent (5) | manifiesto determinista y sellado; exclusión de eventos futuros; permisos; proveedor determinista; confinamiento de rutas |

---

## 7. Decisiones y desviaciones

| Decisión | Motivo | ADR |
|---|---|---|
| Manifiesto guardado completo, hash sobre el sobre sin `content_hash` | Sin esto no se puede reconstruir desde SQLite | 0006 |
| `SimProvider` en vez de proveedor real | Gate sin red ni coste; el contrato es el mismo | 0006 |
| `shell.execute` denegado | No hay sandbox en 0.2; denegar es más honesto que simular | 0006 |
| `branch_revision` = `head_sequence` | Es la revisión materializada de la rama | 0006 |
| `run.start` síncrono en el handler | El proveedor es instantáneo; streaming llega en 0.3 | 0006 |

---

## 8. Huecos y riesgos conocidos

### Alta

1. **Sin sandbox.** Cualquier tool nueva que ejecute código no debe habilitarse hasta que exista aislamiento. Hoy solo hay lectura/escritura confinada al root.
2. **El manifiesto no incluye memoria.** `memories`, `summaries` y `artifact_slices` están vacíos: la selección de contexto es solo historial de eventos. 0.3 debe llenar `memories`.

### Media

3. **Proveedor único y simulado.** No hay handoff ni sesión de proveedor; el receipt declara `sim`.
4. **El export no incluye runs ni manifests.** Un `hliar` restaura eventos y estado de ramas, pero no el historial de ejecución. Deuda declarada para 0.3.
5. **`terminal_event` nulo.** El receipt no puede referenciar su evento de cierre porque se emite después.
6. **Sin streaming.** `model.responded` llega completo; no hay eventos parciales.
7. **Sin límite de tamaño de prompt.** Un prompt enorme se guarda tal cual en `runs.prompt`.

### Baja

8. Los estados `cancelled` y `paused` están en el modelo pero no se ejercitan.
9. No hay `run.cancel` todavía.
10. El receipt no incluye hash del resultado.

---

## 9. Guía de auditoría para Copilot

### Verificar

1. `create_run` fija `branch_revision` y valida que branch pertenece al thread y el thread al workspace.
2. `save_manifest` sella **antes** de insertar y el evento `context.manifest.created` se emite en la misma transacción.
3. El daemon emite `model.requested` solo después de `save_manifest`.
4. `build_manifest` filtra por `sequence <= branch_revision` y por `thread_id`.
5. El proveedor simulado no hace I/O ni red.
6. `execute_tool` confina rutas al root y rechaza `..`.
7. `filesystem.write` no se ejecuta antes de `run.approve`.
8. `finish_run` guarda receipt y emite `run.{state}`.
9. `run.get` devuelve el manifiesto desde SQLite tras reiniciar.
10. `cargo test` pasa desde limpio.

### Preguntas

- ¿Puede un run leer eventos de otra rama del mismo thread? (No: filtra por `branch_revision` del branch del run, no por branch_id; un evento de otra rama con secuencia menor entraría. Es una decisión a revisar en 0.3.)
- ¿El manifiesto incluye el prompt del usuario? (No: el prompt va al provider, no al manifiesto. Debe revisarse si el prompt debe formar parte del contexto reproducible.)
- ¿Qué pasa si el proveedor falla? (Hoy `complete` no devuelve error; en 0.3 debe cubrirse con `run.failed`.)
- ¿El receipt cubre el resultado? (No incluye hash del texto producido.)
- ¿Se puede reejecutar un run con el mismo manifiesto? (No hay `run.replay` todavía.)

### Lo que este informe NO afirma

- No afirma que exista memoria, compactación o retrieval.
- No afirma conformidad MCP.
- No afirma aislamiento de sistema operativo.
- No afirma reproducibilidad de un modelo real: solo del proveedor simulado.

---

## 10. Estado

- 25 pruebas en verde, árbol limpio en `1a484dd`.
- 4 000 líneas de Rust.
- Matriz viva en `docs/CONFORMANCE.md`.
- Siguiente fase: 0.3 (MCP + HLI Memory), no iniciada.
