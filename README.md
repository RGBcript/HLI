# HLI

Harness ligero, local, autónomo y multiproveedor.

**Estado: HLI 0.1 (durabilidad) implementado y verificado.** Las fases 0.2–0.4 no están implementadas todavía; este repositorio no las simula.

## Qué hay en 0.1

- Daemon local como proceso independiente (`hli-daemon`).
- CLI `hli` que habla con el daemon por TCP + NDJSON; nunca escribe en SQLite.
- Workspaces, Threads y Branches de un solo padre.
- Event log append-only con cadena SHA-256 sobre JSON canónico.
- SQLite en WAL con un único actor de escritura.
- Artifact Store por hash SHA-256.
- Snapshots por rama.
- `verify` de la cadena completa.
- `export`/`import` en contenedor `hliar` con verificación de hashes y cadena.

Fuera de 0.1, por diseño: agent loop, ContextManifest, MCP, ACP, memoria, compactación, módulos, MCH. Se implementan en 0.2–0.4.

## Compilar y probar

```bash
cargo build --workspace
cargo test
```

`cargo test` ejecuta 16 pruebas: 10 unitarias del núcleo, 2 del contenedor de export y 4 de extremo a extremo (reinicio real del daemon, export/import, idempotencia, manifiesto manipulado).

## Uso

```bash
export HLI_HOME=/tmp/hli            # o %USERPROFILE%\.hli por defecto
target/debug/hli daemon start
target/debug/hli workspace create demo
target/debug/hli thread create research --workspace <ws>
target/debug/hli event append --type test.recorded --payload '{"n":1}' --workspace <ws> --thread <thr>
target/debug/hli branch create alternative --thread <thr> --from <evt>
target/debug/hli verify --workspace <ws>
target/debug/hli export <ws> --out demo.hliar
target/debug/hli import demo.hliar
target/debug/hli daemon stop
```

Todas las salidas aceptan `--output json|human`.

## Criterio de salida de 0.1

Se cumple cuando un segundo proceso CLI recupera exactamente el workspace, rama, secuencia de eventos, snapshot y artefactos después de reiniciar el daemon. Las pruebas `lifecycle_survives_daemon_restart` y `export_import_roundtrip_is_verifiable` lo verifican matando el proceso y volviendo a arrancarlo.

## Estructura

```
crates/hli-core     modelo, JSON canónico, hash, ids, tiempo
crates/hli-store    SQLite, writer actor, artifact store, snapshots, export/import
crates/hli-daemon   API local TCP+NDJSON
crates/hli-cli      CLI
docs/adr            decisiones de arquitectura
```

## Decisiones

Ver `docs/adr/`. Las relevantes para 0.1:

- ADR-0001: transporte TCP+NDJSON en 0.1 (HTTP/WebSocket en 0.2).
- ADR-0002: el sobre canónico incluye `previous_hash` (corrige §15.8 de la spec).
- ADR-0003: JCS implementado como subconjunto documentado.
- ADR-0004: contenedor `hliar` provisional.
- ADR-0005: alcance estricto de 0.1.
