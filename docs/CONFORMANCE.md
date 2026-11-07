# Matriz de conformidad — HLI

Actualizada al cerrar cada gate. **Estado real del código**, no del plan.

Leyenda: `implementado` · `parcial` · `no` · `no aplica`.

| Sección del draft | 0.1 | 0.2 | 0.3 | 0.4 | Nota |
|---|---|---|---|---|---|
| §2 Modelo conceptual | implementado | implementado | implementado | implementado | MCH separado en profile/instance |
| §3 Arquitectura | parcial | parcial | parcial | implementado | Module SDK y supervisor |
| §4 Persistencia, event log, snapshots | implementado | implementado | implementado | implementado | |
| §4.5 Recuperación de runs | no aplica | implementado | implementado | implementado | |
| §5 Memoria y contexto | no | parcial | implementado | implementado | vigencia aplicada |
| §6 Agent Runtime | no | implementado | implementado | implementado | proveedor simulado |
| §6.4 Autonomía / vía única | no | parcial | parcial | parcial | |
| §7 Tool Runtime | no | parcial | parcial | parcial | read/write; shell denegado |
| §8 Proveedores | no | parcial | parcial | parcial | solo `SimProvider` |
| §9 Permisos | no | parcial | parcial | parcial | allow/ask/deny |
| §9.2 Sandbox | no | no | no | no | supervisor de subproceso, no sandbox de SO |
| §10 CLI | parcial | implementado | implementado | implementado | + module/mch |
| §11 MCP | no | no | implementado | implementado | + mch_invoke/mch_list |
| §11.5–11.6 ACP | no | no | no | no | experimental, fuera de gate |
| §12 Desktop | no | no | no | no | fuera de 0.1–0.4 |
| §13 Module SDK | no | no | no | implementado | protocolo, paquete, supervisor, proyección |
| §14 MCH Memory | no | no | no | implementado | profile/instance/module/endpoint |
| §15.3 Event Model | implementado | implementado | implementado | implementado | |
| §15.4 Branch/merge | parcial | parcial | implementado | implementado | sin merge |
| §15.5 ContextManifest | no | implementado | implementado | implementado | |
| §15.6 Memoria temporal | no | no | parcial | implementado | vigencia en búsqueda |
| §15.7 Receipt | no | implementado | implementado | implementado | |
| §15.8 Canonicalización | implementado | implementado | implementado | implementado | |
| §15.9 Export/import | implementado | implementado | implementado | implementado | + runs, manifests, memorias |
| §16 Roadmap | 0.1 | 0.2 | 0.3 | 0.4 | secuencia completa |

## Gates

| Gate | Prueba | Estado |
|---|---|---|
| 0.1 durabilidad | `e2e::lifecycle_survives_daemon_restart` | cerrado |
| 0.1 export/import | `e2e::export_import_roundtrip_is_verifiable` | cerrado |
| 0.2 contexto y ejecución | `e2e_02::run_manifest_receipt_and_restart` | cerrado |
| 0.2 permisos | `e2e_02::tool_*` | cerrado |
| 0.3 trayectoria y MCP | `scripts/mcp_gate.py` | cerrado |
| 0.4 módulos y MCH | `scripts/module_gate.py` | cerrado |

## Deuda declarada

- Supervisor efímero por invocación; sin pool persistente.
- Paquete sin firma criptográfica (solo checksums).
- Sin WASI; subproceso nativo.
- `shell.execute` denegado; sin sandbox de SO.
- MCP sin `tasks`, `subscriptions/listen` ni recursos binarios.
- Sin `run.cancel`, `run.replay` ni compactación.
- Las pruebas de los gates 0.3 y 0.4 son scripts Python externos, no `cargo test`.
