# Matriz de conformidad — HLI

Actualizada al cerrar cada gate. **Estado real del código**, no del plan.

Leyenda: `implementado` · `parcial` · `no` · `no aplica` (fuera de la fase actual).

| Sección del draft | 0.1 | 0.2 | Nota |
|---|---|---|---|
| §2 Modelo conceptual (W/T/B) | implementado | implementado | MCH no |
| §3 Arquitectura (daemon, capas) | parcial | parcial | Context/Agent/Tool Runtime ya existen; Module SDK no |
| §4 Persistencia, event log, snapshots | implementado | implementado | |
| §4.5 Recuperación de runs | no aplica | implementado | `run.get` tras reinicio |
| §5 Memoria y contexto | no | parcial | `ContextManifest` con eventos e instrucciones; memoria y compactación no |
| §6 Agent Runtime | no | implementado | bucle básico con proveedor simulado |
| §6.4 Autonomía / vía única | no | parcial | el run lo inicia el daemon; sin CLI-to-daemon distinto |
| §7 Tool Runtime | no | parcial | `filesystem.read/write`; `shell` denegado |
| §8 Proveedores | no | parcial | solo `SimProvider` |
| §9 Permisos | no | parcial | allow/ask/deny en tools; sin perfiles ni secretos |
| §9.2 Sandbox | no | no | declarado `none` en el receipt |
| §10 CLI | parcial | implementado | comandos de 0.1 + `run.*`, `manifest.get` |
| §11 MCP | no | no | fase 0.3 |
| §11.5–11.6 ACP | no | no | experimental, no criterio |
| §12 Desktop | no | no | fuera de 0.1–0.4 |
| §13 Module SDK | no | no | fase 0.4 |
| §14 MCH Memory | no | no | fase 0.4 |
| §15.3 Event Model | implementado | implementado | |
| §15.4 Branch/merge | parcial | parcial | un padre; sin merge |
| §15.5 ContextManifest | no | implementado | persistido antes de `model.requested` |
| §15.6 Memoria temporal | no | no | fase 0.3 |
| §15.7 Receipt | no | implementado | forma completa; `terminal_event` nulo |
| §15.8 Canonicalización | implementado | implementado | |
| §15.9 Export/import | implementado | implementado | no incluye runs/manifests todavía |
| §16 Roadmap | 0.1 | 0.2 | secuencia acumulativa |

## Gates

| Gate | Prueba | Estado |
|---|---|---|
| 0.1 durabilidad | `e2e::lifecycle_survives_daemon_restart` | cerrado |
| 0.1 export/import | `e2e::export_import_roundtrip_is_verifiable` | cerrado |
| 0.2 contexto y ejecución | `e2e_02::run_manifest_receipt_and_restart` | cerrado |
| 0.2 permisos | `e2e_02::tool_read_is_allowed`, `tool_write_requires_approval`, `tool_shell_is_denied` | cerrado |
| 0.3 interoperabilidad | — | pendiente |
| 0.4 extensibilidad | — | pendiente |

## Deuda declarada

- Export no incluye `runs` ni `context_manifests`; se añadirá en 0.3 al ampliar el archivo.
- `terminal_event` del receipt queda nulo: el evento `run.completed` se emite después.
- Sin sandbox de sistema operativo; `shell.execute` denegado por política.
