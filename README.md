# HLI

Harness ligero, local, autónomo y multiproveedor.

**Estado: HLI 0.1 (durabilidad) y 0.2 (contexto y ejecución) implementados y verificados.** Las fases 0.3 y 0.4 no están implementadas todavía; este repositorio no las simula.

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

## Qué hay en 0.2

- `Run` fijado a workspace, thread, branch y `branch_revision`.
- `ContextManifest` inmutable, persistido antes de `model.requested`.
- Proveedor simulado determinista, sin red ni coste.
- Agent loop básico: contexto → model.requested → model.responded → tool opcional → fin.
- `Reproducibility Receipt` por run.
- Permisos `allow`, `ask`, `deny`; Tool Runtime con `filesystem.read` y `filesystem.write` confinadas al root del workspace.
- Recuperación de runs y manifiestos tras reiniciar el daemon.

Fuera de 0.2, por diseño: MCP, ACP, memoria, compactación, módulos, MCH y sandbox de sistema operativo (`shell` denegado por política). Se implementan en 0.3–0.4.

## Compilar y probar

```bash
cargo build --workspace
cargo test
```

`cargo test` ejecuta 25 pruebas: 10 unitarias del núcleo, 2 del contenedor de export, 5 del agent runtime y 8 de extremo a extremo (0.1: reinicio, export/import, idempotencia, manifiesto manipulado; 0.2: run+manifiesto+receipt, permisos read/write/shell).

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
target/debug/hli run start --workspace <ws> --thread <thr> --branch <br> --prompt "hola"
target/debug/hli run get <run>
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
- ADR-0006: run, ContextManifest y agent loop de 0.2.
