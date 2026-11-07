# ADR-0005 — Alcance estricto de HLI 0.1

**Estado:** aceptada · **Fecha:** 2026-11-07

## Decisión

0.1 implementa solo durabilidad:

- daemon + CLI,
- workspace/thread/branch,
- event log + hash chain,
- SQLite WAL + writer actor,
- artifact store,
- snapshots,
- verify,
- export/import.

No implementa: agent loop, ContextManifest, memoria, compactación, MCP, ACP, módulos, MCH,
Desktop. No hay simulaciones de esas capacidades.

## Por qué

La especificación establece que las fases son una secuencia acumulativa y que ningún
incremento debe introducir una interfaz que la fase siguiente deba sustituir. Las interfaces
internas de 0.1 que 0.2 reutiliza son:

- `Event` y su cadena (`hli-core::model`).
- `Writer`/`Reader` (`hli-store`), con el actor como única vía de escritura.
- El contrato `cmd`/`args`/`data` del daemon.
- El contenedor `hliar`.

## Consecuencia

0.2 añade `run`, ContextManifest y el bucle del agente sobre estas mismas interfaces, sin
reescribir el almacén.
