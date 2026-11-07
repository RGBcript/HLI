# ADR-0007 — Trayectoria de rama, prompt cubierto, memoria y MCP

**Estado:** aceptada · **Fecha:** 2026-11-07

## Contexto

El informe de 0.2 identificó dos defectos: el manifiesto seleccionaba eventos por
`thread_id + sequence <= branch_revision`, lo que permitía incluir ramas hermanas, y el prompt
no formaba parte del manifiesto, así que no quedaba cubierto por `content_hash`.

## Decisiones

1. **Trayectoria visible.** Un evento es visible para una rama si pertenece a su cadena de
   ramas ancestrales, con el límite de `fork_event` de cada hija y `branch_revision` de la
   rama actual. Los eventos del thread sin rama (creación) entran hasta la revisión.
   `parent_branch_id` se fija desde el `branch_id` del evento de bifurcación.
2. **Prompt cubierto.** `ContextManifest.user_input = {content, content_hash}`. El proveedor
   consume `manifest.user_input.content`, no el prompt suelto del run.
3. **Memoria.** Tabla `memories` + `memories_fts` (FTS5, `unicode61 remove_diacritics 2`).
   Tipos y estados cerrados; `commit`, `update`, `forget`, `get`, `search`. Las memorias
   activas relevantes al prompt se incluyen en `manifest.memories` (top 5 por BM25).
4. **MCP baseline.** Servidor `hli-mcp` sobre stdio con framing `Content-Length`.
   `initialize` (2025-11-25), `server/discover` (moderno), `tools/list`, `tools/call`,
   `resources/list`, `resources/read`, `prompts/list`. Sin estado de protocolo.
5. **Receipt finalizado.** `finish_run` emite `run.{state}` y, en la misma transacción,
   escribe `receipt.terminal_event` con el id de ese evento. Sin referencias circulares.
   Se añade `result_hash`.

## Consecuencias

- El gate de 0.3 se ejecuta con `scripts/mcp_gate.py`, un cliente MCP externo real.
- `export` incluye `runs.ndjson`, `manifests.ndjson` y `memories.ndjson`; `import` los
  reconstruye y reindexa FTS.
- No hay embeddings: la búsqueda es léxica, como exige 0.3.
- El servidor MCP no expone administración del daemon: solo las tools declaradas.
