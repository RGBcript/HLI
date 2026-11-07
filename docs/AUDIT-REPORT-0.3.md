# HLI — Informe de fase 0.3

**Fase:** HLI 0.3 — interoperabilidad (MCP y HLI Memory)
**Estado:** cerrada y verificada
**Especificación:** §5, §11, §15.4–15.9, §16.4
**Audiencia:** Copilot (auditor externo)
**Fecha:** 2026-11-07

---

## 1. Resumen ejecutivo

0.3 cierra dos defectos de 0.2 y añade las dos piezas que faltaban para interoperar: memoria
léxica y servidor MCP.

**Demostrado con el gate ejecutable `scripts/mcp_gate.py`**, que es un cliente MCP externo real
(JSON-RPC 2.0 sobre stdio con framing `Content-Length`, proceso separado):

- trayectoria visible de rama correcta (ancestro incluido, hermana y descendiente excluidos,
  evento posterior a la revisión excluido);
- prompt cubierto por `ContextManifest.user_input` y por `content_hash`;
- memoria registrada, recuperada por FTS5 e incluida en el manifiesto del run;
- receipt finalizado con `terminal_event` y `result_hash`;
- cliente MCP que descubre 7 tools, llama tools, lee recursos y se reconecta;
- export/import que conserva memoria, run, manifiesto y receipt.

**No implementado en 0.3:** embeddings, compactación, sandbox, ACP, `tasks`, `run.cancel` y
binarios en `artifact.read`.

---

## 2. Correcciones obligatorias

### 2.1 Trayectoria visible de rama

`Reader::visible_events(branch_id, revision)` construye la cadena de ramas subiendo por
`parent_branch_id`, con el límite de `fork_event` de cada hija, y añade los eventos del thread
sin rama (creación) hasta la revisión. `create_branch` fija `parent_branch_id` desde el
`branch_id` del evento de bifurcación.

Pruebas ejecutadas en el gate:

| Assert | Qué verifica |
|---|---|
| `ancestor_event_is_included` | un evento de la rama A aparece en el run de B, hija de A |
| `sibling_branch_event_is_excluded` | un evento de C, hermana de B, no aparece en B |
| `descendant_event_is_excluded` | un evento de B no aparece en el run de A |
| `post_revision_event_is_excluded` | un evento añadido después de crear el run no entra |

### 2.2 Prompt reproducible

`ContextManifest.user_input = {content, content_hash}`. El proveedor consume
`manifest.user_input.content`, no el prompt suelto. Cambiar el prompt cambia el hash del
manifiesto (probado en `prompt_is_covered_by_manifest_hash`, unitaria del agent).

---

## 3. HLI Memory

- Tabla `memories` + `memories_fts` (FTS5, `unicode61 remove_diacritics 2`).
- Tipos: `fact`, `decision`, `constraint`, `summary`, `reference`, `procedure`.
- Estados: `proposed`, `active`, `superseded`, `revoked`, `deleted_by_policy`.
- Relaciones `supersedes` y `contradicts`.
- Ámbitos: `global`, `workspace`, `thread`, `branch`.
- Operaciones: `memory.commit`, `memory.get`, `memory.search`, `memory.update`, `memory.forget`.
- Las memorias activas relevantes al prompt (top 5 por BM25) entran en `manifest.memories`.
- No hay embeddings: la búsqueda es léxica, como exige la fase.

---

## 4. MCP

Servidor `hli-mcp` sobre stdio, framing `Content-Length`, sin estado de protocolo.

| Superficie | Contenido |
|---|---|
| Handshake | `initialize` (2025-11-25) y `server/discover` (moderno) |
| Tools | `workspace_list`, `thread_read`, `memory_search`, `memory_commit`, `run_start`, `run_get`, `artifact_read` |
| Resources | `hli://workspace/<id>`, `hli://thread/<id>`, `hli://memory/<id>`, `hli://artifact/<id>`, `hli://run/<id>` |
| Prompts | `hli.plan`, `hli.review` |

El servidor no administra el daemon: solo expone las tools declaradas. Es la frontera que pide
§15.1.

---

## 5. Receipt y export

- `finish_run` emite `run.{state}` y, en la misma transacción, escribe
  `receipt.terminal_event` con el id de ese evento. Sin referencia circular.
- `receipt.result_hash` cubre el resultado del run.
- `export` añade `runs.ndjson`, `manifests.ndjson` y `memories.ndjson`; `import` los reconstruye
  y reindexa FTS. El gate verifica que memoria, run y `content_hash` sobreviven al ciclo.

---

## 6. Gate de 0.3: evidencia

```
$ python scripts/mcp_gate.py
{"gate": "0.3", "ok": true,
 "tools": ["workspace_list","thread_read","memory_search","memory_commit",
           "run_start","run_get","artifact_read"],
 "memories": 1, "run": "run_db4b619a0582a2bb",
 "manifest": "ctx_4df8cbe64afd16b7"}
```

El script crea un `HLI_HOME` temporal, arranca el daemon como proceso, crea workspace, thread,
tres ramas (A, B, C), ejecuta los cuatro asserts de visibilidad, registra memoria, lanza runs,
abre `hli-mcp`, ejecuta el ciclo MCP, cierra, exporta, importa en otro home y verifica.

---

## 7. Huecos y riesgos

### Alta

1. **Sin sandbox.** `shell.execute` sigue denegado. Cualquier tool nueva de ejecución requiere
   aislamiento antes de habilitarse.
2. **Memoria sin invalidación temporal.** `valid_from`/`valid_until` existen, pero `search` no
   filtra por vigencia; una memoria superseded se excluye por estado, no por tiempo.

### Media

3. **`artifact.read` solo texto.** Un PNG o PDF devuelve error; 0.4 debe servir binario.
4. **MCP sin `tasks` ni `subscriptions/listen`.** `run.start` es síncrono; un run largo
   bloquearía la llamada.
5. **Sin `run.cancel`.** El estado existe en el modelo, no la operación.
6. **FTS5 sin trigram.** Subcadenas y CJK pueden fallar; el draft lo preveía como índice
   adicional.
7. **El gate es un script Python, no `cargo test`.** Es un cliente externo real, pero no se
   ejecuta con `cargo test`.

### Baja

8. El servidor MCP no valida `Origin` (no aplica a stdio).
9. `prompts/list` devuelve plantillas sin argumentos.
10. `thread_read` lista threads; no hay `thread.get` en MCP (sí en el daemon).

---

## 8. Guía de auditoría para Copilot

### Verificar

1. `visible_events` sube por `parent_branch_id` y corta en `fork_event`.
2. `create_branch` fija `parent_branch_id` desde el evento.
3. `build_manifest` recibe eventos ya visibles y no vuelve a filtrar por thread.
4. `user_input` está en el sobre y el sello lo cubre.
5. El proveedor lee `manifest.user_input.content`.
6. `memory.search` usa FTS5 y filtra `status = 'active'`.
7. `memory.commit` inserta en `memories` y `memories_fts` en la misma transacción.
8. `finish_run` escribe `terminal_event` tras emitir el evento, en la misma transacción.
9. `hli-mcp` no abre SQLite: habla con el daemon por TCP.
10. `import` reindexa FTS y conserva `content_hash`.

### Preguntas

- ¿Un evento de una rama hermana con `sequence` menor puede colarse? (No: se filtra por
  `branch_id` de la cadena).
- ¿El `fork_event` de una hija corta correctamente los eventos del padre? (Sí, por secuencia).
- ¿Qué pasa si una rama se crea sin `from`? (Parent nulo; solo ve sus eventos y los del thread).
- ¿El manifiesto cubre las memorias? (Sí, `memories[]` dentro del sobre).
- ¿El receipt referencia un evento que existe? (Sí, `run.completed`).
- ¿El import duplica memorias si se ejecuta dos veces? (No: `INSERT OR IGNORE` por id).

### Lo que este informe NO afirma

- No afirma conformidad MCP universal: solo el baseline probado con el script.
- No afirma aislamiento de sistema operativo.
- No afirma embeddings ni búsqueda semántica.
- No afirma que `cargo test` cubra 0.3: el gate es `scripts/mcp_gate.py`.

---

## 9. Estado y siguiente fase

- `cargo test`: 25 pruebas en verde.
- Gate 0.3: `scripts/mcp_gate.py` en verde.
- Matriz viva: `docs/CONFORMANCE.md`.
- Siguiente: 0.4 (Module Protocol, `.hlimod`, MCH), no iniciada.
