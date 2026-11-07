# Matriz de conformidad — HLI

Actualizada al cerrar cada gate. **Estado real del código**, no del plan.

Leyenda: `implementado` · `parcial` · `no` · `no aplica`.

| Sección del draft | 0.1 | 0.2 | 0.3 | Nota |
|---|---|---|---|---|
| §2 Modelo conceptual (W/T/B) | implementado | implementado | implementado | MCH no |
| §3 Arquitectura | parcial | parcial | parcial | falta Module SDK |
| §4 Persistencia, event log, snapshots | implementado | implementado | implementado | |
| §4.5 Recuperación de runs | no aplica | implementado | implementado | |
| §5 Memoria y contexto | no | parcial | implementado | memoria + manifiesto; compactación no |
| §6 Agent Runtime | no | implementado | implementado | bucle básico, proveedor simulado |
| §6.4 Autonomía / vía única | no | parcial | parcial | |
| §7 Tool Runtime | no | parcial | parcial | read/write; shell denegado |
| §8 Proveedores | no | parcial | parcial | solo `SimProvider` |
| §9 Permisos | no | parcial | parcial | allow/ask/deny en tools |
| §9.2 Sandbox | no | no | no | `sandbox: none` en receipt |
| §10 CLI | parcial | implementado | implementado | + `memory.*` |
| §11 MCP | no | no | implementado | stdio, 7 tools, 5 recursos |
| §11.5–11.6 ACP | no | no | no | experimental, fuera de gate |
| §12 Desktop | no | no | no | fuera de 0.1–0.4 |
| §13 Module SDK | no | no | no | fase 0.4 |
| §14 MCH Memory | no | no | no | fase 0.4 |
| §15.3 Event Model | implementado | implementado | implementado | |
| §15.4 Branch/merge | parcial | parcial | implementado | trayectoria ancestral; sin merge |
| §15.5 ContextManifest | no | implementado | implementado | + `user_input` y memorias |
| §15.6 Memoria temporal | no | no | parcial | tipos/estados/relaciones; sin invalidación temporal |
| §15.7 Receipt | no | implementado | implementado | `terminal_event` y `result_hash` |
| §15.8 Canonicalización | implementado | implementado | implementado | |
| §15.9 Export/import | implementado | implementado | implementado | + runs, manifests, memorias |
| §16 Roadmap | 0.1 | 0.2 | 0.3 | secuencia acumulativa |

## Gates

| Gate | Prueba | Estado |
|---|---|---|
| 0.1 durabilidad | `e2e::lifecycle_survives_daemon_restart` | cerrado |
| 0.1 export/import | `e2e::export_import_roundtrip_is_verifiable` | cerrado |
| 0.2 contexto y ejecución | `e2e_02::run_manifest_receipt_and_restart` | cerrado |
| 0.2 permisos | `e2e_02::tool_read_is_allowed`, `tool_write_requires_approval`, `tool_shell_is_denied` | cerrado |
| 0.3 trayectoria de rama | `scripts/mcp_gate.py` (4 asserts) | cerrado |
| 0.3 MCP + memoria + reconexión + import | `scripts/mcp_gate.py` | cerrado |
| 0.4 extensibilidad | — | pendiente |

## Deuda declarada

- Sin sandbox de sistema operativo; `shell.execute` denegado.
- Sin compactación ni embeddings (fuera de 0.3).
- `artifact.read` solo sirve texto; binarios quedan para 0.4.
- El servidor MCP no implementa `tasks` ni `subscriptions/listen`.
- Sin `run.cancel` ni `run.replay`.
- Las pruebas de trayectoria de rama viven en `scripts/mcp_gate.py`, no en `cargo test`.
