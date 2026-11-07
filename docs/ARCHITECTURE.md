# HLI — Arquitectura

HLI separa cuatro capas:

1. **Almacén** (`hli-store`): SQLite en WAL, actor único de escritura, event log
   append-only con cadena de hashes, proyecciones (workspaces, threads, branches,
   runs, memorias, módulos, MCH).
2. **Daemon** (`hli-daemon`): API local TCP+NDJSON, autoridad de estado, ejecuta
   runs y supervisa MCH.
3. **Clientes**: CLI (`hli`), TUI, MCP (`hli-mcp`) y SDK interno (`hli-agent`).
4. **Extensiones**: módulos `.hlimod` como procesos JSON-RPC sobre stdio (MCH),
   firmados opcionalmente con Ed25519.

## Flujo de un run

CLI → daemon → contexto (memorias, artifacts, slices) → proveedor → eventos
`run.*` → receipt → manifest de contexto. Los eventos se sellan con SHA-256 y
`prev_hash`; el store verifica la cadena.

## Persistencia

- `HLI_HOME/hli.db` con migraciones versionadas 0001–0007.
- `HLI_HOME/artifacts/`, `snapshots/`, `backups/`.
- Secretos solo en `HLI_HOME/secrets/` o variables `HLI_SECRET_*`; nunca en la
  base, eventos, logs ni exportaciones.

## MCH

Un proceso por instancia, lifecycle `initialize → capabilities → configure →
health`, health periódico, degraded, reinicio automático acotado y autostart
persistente. Ver `docs/MCH-GUIDE.md`.

## Límites

No hay sandbox del sistema operativo, routing económico ni garantía de
compatibilidad MCP universal. `shell.execute` está denegado.
