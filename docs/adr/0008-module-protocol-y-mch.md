# ADR-0008 — Module Protocol, .hlimod y MCH de 0.4

**Estado:** aceptada · **Fecha:** 2026-11-07

## Decisiones

1. **Module Protocol propio, separado de MCP.** JSON-RPC 2.0 delimitado por línea sobre stdio.
   Métodos: `initialize`, `capabilities`, `configure`, `invoke`, `cancel`, `health`,
   `checkpoint`, `shutdown`. Cada request lleva `_meta` con `workspaceId`, `requestId` e
   `idempotencyKey`. El módulo no abre `hli.db`, no lee secretos y no expone MCP: delega en el
   daemon por TCP.
2. **Paquete `.hlimod`** con el contenedor `HLIAR1` ya existente: `manifest.json`, `schemas/`,
   `bin/`, `checksums.json`. Se eligió JSON en lugar de YAML para no añadir dependencia;
   el contenido es equivalente.
3. **Supervisor de subproceso por invocación.** `mch.invoke` lanza el binario, hace
   `initialize` → `invoke` → `shutdown`, con timeout de 30 s y kill al vencer. No hay pool
   persistente ni carga dinámica. `mch.serve` valida con un health check y deja la instancia
   en `ready`.
4. **Proyección única.** La operación del manifiesto se expone como comando CLI
   (`hli mch invoke`) y como tool MCP (`mch_invoke`). La validación de operaciones declaradas
   vive en el daemon, no duplicada.
5. **MCH.** `mch_profiles` (dominio y módulo) y `mch_instances` (workspace, estado, pid) son
   tablas separadas. Un MCH no es un servidor MCP: es una instancia que se proyecta por CLI
   y MCP.
6. **MCH Memory** reutiliza `memories` y `memories_fts`; no hay segunda base.

## Consecuencias

- El gate 0.4 se ejecuta con `scripts/module_gate.py`.
- `artifact.read` devuelve texto UTF-8 o hex, con límite de 1 MiB.
- `memory.search` filtra por vigencia (`valid_from <= now`, `valid_until` nulo o futuro).
- Deuda: supervisor efímero (sin pool), sin firma criptográfica del paquete, sin WASI.
