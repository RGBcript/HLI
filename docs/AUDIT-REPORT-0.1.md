# HLI — Informe de auditoría técnica

**Artefacto:** HLI 0.1 (fase de durabilidad) y plan de fases 0.2–0.4
**Repositorio:** `C:\Users\GUERRA\Documents\TEC\HLI`
**Commit auditado (código):** `fae01d4` · el informe se añade en un commit posterior sobre ese árbol
**Especificación de referencia:** `HLI_Specification_Draft_v0.14.docx`
**Fecha:** 2026-11-07
**Audiencia:** Copilot (auditor externo)
**Autor:** implementación asistida por Hermes; decisiones registradas en `docs/adr/`

---

## 1. Resumen ejecutivo

HLI es un runtime agéntico local y durable. El repositorio implementa **solo la fase 0.1**: daemon local, workspaces/threads/branches, event log append-only con cadena de hashes, SQLite en WAL con un único actor de escritura, artifact store, snapshots, verificación de integridad y exportación/importación verificable.

**Lo que está demostrado con pruebas ejecutables:** el criterio de salida de 0.1. Un daemon real se mata, se relanza y un segundo proceso CLI recupera el mismo workspace, rama, secuencia, snapshot y artefactos; `verify` devuelve `ok` con el mismo `chain_head`; un archivo exportado se importa en otro `HLI_HOME` y se verifica de nuevo.

**Lo que NO está implementado:** 0.2 (ContextManifest, runs, agent loop, permisos, sandbox), 0.3 (MCP, HLI Memory), 0.4 (Module Protocol, `.hlimod`, MCH). No hay stubs que simulen esas capacidades.

**Veredicto de madurez:** sustrato de durabilidad sólido para su tamaño (2 801 líneas de Rust, 16 pruebas). No es todavía un harness: es el cimiento sobre el que se construye el harness. Un auditor debe tratar las afirmaciones de 0.2–0.4 como **plan**, no como estado.

---

## 2. Cómo reproducir la auditoría

Requisitos: Rust 1.99+ (host `x86_64-pc-windows-msvc`), sin servicios externos.

```bash
cd C:\Users\GUERRA\Documents\TEC\HLI
cargo build --workspace
cargo test
```

Resultado esperado (real, no estimado):

```
test result: ok. 10 passed   (hli-core)
test result: ok.  2 passed   (hli-store)
test result: ok.  4 passed   (hli-cli, e2e)
```

Ciclo manual del gate:

```bash
export HLI_HOME=C:/tmp/hli-audit        # ruta nativa, no MSYS
target/debug/hli daemon start
target/debug/hli workspace create demo
target/debug/hli thread create research --workspace <ws>
target/debug/hli event append --type test.recorded --payload '{"n":1}' \
    --workspace <ws> --thread <thr>
target/debug/hli branch create alternative --from <evt>
target/debug/hli branch show <br> --output human
target/debug/hli verify --workspace <ws> --output human
target/debug/hli daemon stop
target/debug/hli daemon start          # el mismo HLI_HOME
target/debug/hli branch show <br> --output human   # misma secuencia y head
target/debug/hli export <ws> --out demo.hliar
target/debug/hli daemon stop
export HLI_HOME=C:/tmp/hli-audit-2     # home distinto
target/debug/hli daemon start
target/debug/hli import demo.hliar
target/debug/hli verify --workspace <ws>
```

---

## 3. Arquitectura implementada

### 3.1 Crates y dirección de dependencias

```
hli-core   → modelo, JSON canónico, SHA-256, ids, tiempo   (no conoce SQLite)
hli-store  → SQLite, actor de escritura, artifacts, export  (depende de core)
hli-daemon → API local TCP+NDJSON                            (depende de store)
hli-cli    → cliente del daemon                              (no depende de store)
```

El CLI **no abre SQLite**. Si lo hiciera habría dos escritores y el actor dejaría de ser la única vía. La regla se verifica leyendo `crates/hli-cli/Cargo.toml`: no incluye `rusqlite` ni `hli-store`.

### 3.2 Flujo de una escritura

```
CLI ──NDJSON──▶ daemon ──WriteCmd──▶ writer actor (1 hilo, 1 Connection)
                                         │
                                         ├─ BEGIN IMMEDIATE
                                         ├─ sequence = MAX(sequence)+1
                                         ├─ prev_hash = último event_hash
                                         ├─ JCS(evento) → SHA-256 → event_hash
                                         ├─ INSERT event
                                         ├─ UPDATE branch.head
                                         └─ COMMIT
```

Lectores: conexión propia con `query_only=ON`. Nunca comparten la conexión del actor.

### 3.3 Componentes de 0.1

| Componente | Estado | Dónde |
|---|---|---|
| Daemon local | implementado | `hli-daemon` |
| CLI JSON/JSONL | implementado | `hli-cli` |
| Workspaces / Threads / Branches | implementado | `hli-store` |
| Event log append-only | implementado | `hli-store::schema` + `writer` |
| Cadena de integridad | implementado | `hli-core::canon` + `model` |
| Writer Actor | implementado | `hli-store::writer` |
| Artifact Store | implementado | `hli-store::writer::put_artifact` |
| Snapshots | implementado | `hli-store::writer::snapshot` |
| `verify` | implementado | `hli-store::reader::verify` |
| Export / import | implementado | `hli-store::writer` + `export` |
| Context Engine | **no** | fase 0.2 |
| Agent Runtime | **no** | fase 0.2 |
| Tool Runtime | **no** | fase 0.2 |
| MCP host/server | **no** | fase 0.3 |
| Module SDK | **no** | fase 0.4 |
| MCH | **no** | fase 0.4 |

---

## 4. Contratos implementados

### 4.1 Event Model

```json
{
  "event_id": "evt_...",
  "workspace_id": "ws_...",
  "thread_id": "thr_...",
  "branch_id": "br_...",
  "run_id": null,
  "causation_id": null,
  "correlation_id": null,
  "idempotency_key": "idem_...",
  "sequence": 4,
  "type": "branch.created",
  "schema_version": 1,
  "created_at": "2026-11-07T08:08:25.123Z",
  "actor": {"kind": "user", "id": "cli"},
  "payload": {"name": "alternative", "from_event": "evt_..."},
  "artifacts": [],
  "integrity": {
    "canonicalization": "RFC8785",
    "hash_alg": "sha256",
    "previous_hash": "sha256:...",
    "event_hash": "sha256:..."
  }
}
```

**Regla de hash (ADR-0002):** el sobre canónico es el evento **sin `event_hash`**, pero **con `previous_hash`**. La especificación original decía «sin `integrity`»; eso dejaba el enlace anterior fuera del hash y permitía alterar `previous_hash` sin detección. Se corrigió en código y en el §15.8 del draft.

`event_hash = "sha256:" + hex(SHA-256(JCS(sobre)))`; el primer evento usa 64 ceros como `previous_hash`. `sequence` es monotónica por workspace y la asigna el actor dentro de `BEGIN IMMEDIATE`.

### 4.2 Esquema SQLite (real)

Tablas: `schema_migrations`, `workspaces`, `threads`, `branches`, `events`, `artifacts`, `snapshots`.

Invariantes en la base, no solo en código:

- `UNIQUE(workspace_id, sequence)`.
- `UNIQUE(workspace_id, idempotency_key)` — `NULL` permitido y no colisiona.
- `UNIQUE(thread_id, name)` en branches.
- Triggers `BEFORE UPDATE` / `BEFORE DELETE` sobre `events` con `RAISE(ABORT, 'events are append-only')`.
- `PRAGMA journal_mode=WAL`, `synchronous=NORMAL`, `foreign_keys=ON`, `busy_timeout=5000`.

DDL completo en `crates/hli-store/src/schema.rs`.

### 4.3 API local

TCP en `127.0.0.1:0` (puerto efímero). Publica `daemon.json` con `{version,pid,port,home,started_at,transport}`.

Petición: `{"id":"1","cmd":"workspace.create","args":{...}}`
Respuesta: `{"id":"1","ok":true,"data":{...}}` o `{"id":"1","ok":false,"error":{"code","message"}}`.

Comandos: `daemon.ping`, `daemon.stop`, `workspace.create|list|get`, `thread.create|list`, `branch.create|list|show|snapshot`, `event.append|list`, `artifact.put|list`, `verify`, `export`, `import`, `status`.

### 4.4 Contenedor de exportación

```
HLIAR1\n
FILE <len> manifest.json\n<bytes>\n
FILE <len> state.json\n<bytes>\n
FILE <len> events.ndjson\n<bytes>\n
FILE <len> artifacts/<sha256>\n<bytes>\n
```

- `manifest.json` incluye `format`, `workspace_id`, `exported_at`, `sequence_range`, `chain_head`, `counts`, `files[{path,sha256,bytes}]` y `redactions`.
- `exported_at` queda fuera del hash del manifiesto.
- `events.ndjson`: una línea por evento, ordenada por `sequence`, en JCS.
- `import` verifica hashes de archivo, cadena completa y `chain_head` **antes** de escribir; si algo falla, no escribe nada.

### 4.5 Artefactos

Contenido direccionado por SHA-256. `artifact put` copia a `artifacts/<sha>`, inserta metadatos y emite `artifact.created` con `artifacts[].sha256`. El binario nunca entra al event log.

### 4.6 Snapshots

`branch.create` genera un snapshot **obligatorio** (ADR: requisito de §16.1), lo escribe en `snapshots/<branch>/<sequence>.json` y lo enlaza en `branches.snapshot_id`. El snapshot contiene rama, thread, workspace y evento head; **no contiene el filesystem del workspace**.

---

## 5. Fase 0.1 — estado y evidencia

### 5.1 Alcance

Daemon + CLI; workspace/thread/branch de un solo padre; event log; SQLite WAL + actor; artifact store; snapshots; verify; export/import.

### 5.2 Criterio de salida

> «HLI 0.1 se acepta cuando un segundo proceso CLI puede recuperar exactamente el workspace, rama, secuencia de eventos, snapshot y artefactos después de reiniciar el daemon.»

### 5.3 Evidencia

| Prueba | Qué demuestra |
|---|---|
| `lifecycle_survives_daemon_restart` | Mata el proceso del daemon, arranca otro, compara `head_sequence`, `chain_head`, nombre de rama, `snapshot_id`, existencia del archivo de snapshot y `verify` con 4 eventos |
| `export_import_roundtrip_is_verifiable` | Exporta 5 eventos + 1 artefacto; importa en otro `HLI_HOME`; verifica `chain_head`, `verify` y estado de la rama |
| `append_is_idempotent` | Misma `idempotency_key` devuelve el mismo `event_id` y `sequence`, sin duplicar |
| `import_rejects_tampered_manifest` | Alterar `chain_head` en el manifiesto hace fallar el import con `conflict` |
| `tampering_payload_breaks_hash` (unit) | Cambiar el payload invalida `event_hash` |
| `previous_hash_is_covered` (unit) | Cambiar `previous_hash` cambia el hash calculado |

Las pruebas e2e arrancan el daemon **como proceso independiente** desde el test, con `Stdio::null()`, y cada una usa su propio `HLI_HOME`. El reinicio es real: `child.wait()` sobre el proceso muerto.

### 5.4 Desviaciones y decisiones de 0.1

| Desviación | Motivo | ADR |
|---|---|---|
| Transporte TCP+NDJSON en vez de HTTP/WebSocket | 0.1 no necesita HTTP; el contrato `cmd/args/data` se conserva | 0001 |
| Sobre canónico incluye `previous_hash` | La spec dejaba el enlace fuera del hash | 0002 |
| JCS como subconjunto documentado | Sin dependencia extra; cubre el rango usado | 0003 |
| Contenedor `hliar` propio | Evita zip/zstd en 0.1; el contenido es el mismo | 0004 |
| `branch.name` añadido al modelo | La CLI de la spec usa nombre (`branch create alternative`) | 0005 |
| `--thread` opcional si hay `--from` | Alinea con la CLI de la spec | — |

---

## 6. Fase 0.2 — plan (no implementada)

**Objetivo:** contexto y ejecución. Nada de esto existe todavía en el código.

| Componente | Contrato previsto | Depende de |
|---|---|---|
| `run` | Run fijado a workspace/thread/branch/branch_revision; estados | event log |
| ContextManifest | Persistido **antes** de `model.requested`; hash; exclusiones con razón | event log |
| Agent loop | modelo → tool call → observación → finalización; límites | Tool Runtime |
| Tool Runtime | `filesystem` y `shell` con permisos por capacidad | sandbox |
| Provider Session | handoff explícito al cambiar modelo/proveedor | receipt |
| Reproducibility Receipt | provider, modelo, manifiesto, permisos, sandbox, artefactos | run |
| Compactación | umbral, tail verbatim, resumen incremental como memoria `summary` | ContextManifest |
| Retrieval | FTS5/BM25 determinista, `query_hash` en el manifiesto | memoria |
| Sandbox | Job Object / Landlock+bwrap / Seatbelt; perfil aplicado en el receipt | SO |

**Criterio de salida propuesto:** un run con un proveedor simulado produce un `ContextManifest` persistido, un receipt completo y una salida reproducible en condiciones; el manifiesto permite reconstruir exactamente qué se envió.

**Riesgo principal:** el agent loop es donde se pierde la disciplina de 0.1 si se permite que el runtime escriba SQLite directamente o que el manifiesto se genere después del request. La invariante debe probarse, no documentarse.

---

## 7. Fase 0.3 — plan (no implementada)

**Objetivo:** interoperabilidad.

| Componente | Estado previsto |
|---|---|
| MCP host | baseline 2026-07-28; detección de era `planned` |
| MCP server | `server/discover`, tools/resources/prompts, sin sesiones de protocolo |
| Extensión `tasks` | opcional; `run.start` devuelve task si el cliente la declara |
| HLI Memory | memoria con tipos/ámbitos, FTS5, `memory.search/commit` |
| Recursos | `hli://workspace|thread|artifact|run` |
| Políticas de exposición | qué se publica y con qué permisos |
| ACP | **adaptador opcional, experimental**; no es criterio de 0.3 |

**Criterio de salida propuesto:** un cliente MCP moderno lista herramientas, crea un thread y lanza un run; un cliente 2025-11-25 solo se declara compatible si pasa la prueba legacy.

**Riesgo principal:** prometer conformidad MCP sin pruebas. El draft lo marca `baseline implementable`, no `normative`; el informe mantiene esa distinción.

---

## 8. Fase 0.4 — plan (no implementada)

**Objetivo:** extensibilidad.

| Componente | Estado previsto |
|---|---|
| Module Protocol | `module/discover` sin handshake; invoke/cancel/health/checkpoint/shutdown |
| Paquetes `.hlimod` | manifiesto, esquemas, binario, firma, lockfile |
| Aislamiento | subproceso con sandbox del SO; WASI solo módulos puros |
| Proyección | misma operación como CLI y como tool MCP |
| MCH | Profile / Instance / Process / Endpoint sobre el mismo core |
| Pruebas de contrato | instalar sin tocar el core; permisos no declarados fallan |

**Criterio de salida propuesto:** un módulo de ejemplo (memory) se instala, se proyecta en CLI y MCP, sobrevive a reinicio y no puede ampliar permisos.

**Riesgo principal:** el protocolo de módulos es la superficie más fácil de sobreingenierizar. La regla del draft es que no acceda a SQLite ni se convierta en un segundo servidor MCP.

---

## 9. Matriz de conformidad: especificación vs implementación

| Sección del draft | Estado real | Nota |
|---|---|---|
| §2 Modelo conceptual | parcial | W/T/B implementados; MCH no |
| §3 Arquitectura | parcial | daemon y 4 crates; Context/Agent/Tool Runtime no |
| §4 Persistencia y recuperación | implementado | WAL, actor, snapshots, migraciones |
| §4.5 Recuperación de runs | no aplica | no hay runs en 0.1 |
| §5 Memoria y contexto | no | fase 0.2 |
| §6 Agent Runtime | no | fase 0.2 |
| §7 Tool Runtime | no | fase 0.2 |
| §8 Proveedores | no | fase 0.2 |
| §9 Permisos y secretos | no | fase 0.2 |
| §9.2 Sandbox | no | planned en el draft |
| §10 CLI | parcial | comandos de 0.1; sin TUI |
| §11 MCP | no | fase 0.3 |
| §11.5–11.6 ACP | no | experimental, no criterio |
| §12 Desktop | no | fuera de 0.1–0.4 |
| §13 Module SDK | no | fase 0.4 |
| §14 MCH Memory | no | fase 0.4 |
| §15.3 Event Model | implementado | con la corrección de ADR-0002 |
| §15.4 Branch/merge | parcial | branch de un padre; sin merge |
| §15.5 ContextManifest | no | fase 0.2 |
| §15.6 Memoria temporal | no | fase 0.2/0.3 |
| §15.7 Receipt | no | fase 0.2 |
| §15.8 Canonicalización | implementado | ADR-0002 y 0003 |
| §15.9 Export | implementado | contenedor `hliar` |
| §16 Roadmap | 0.1 de 4 | secuencia acumulativa |

---

## 10. Huecos y riesgos conocidos

### Severidad alta

1. **Sin autenticación local.** Cualquier proceso del usuario puede conectarse al socket y ejecutar comandos. En 0.1 el daemon es de un solo usuario, pero la superficie existe. Mitigación futura: token en `daemon.json` con permisos de archivo, o named pipe con ACL.
2. **Truncamiento no detectable.** La cadena verifica el orden, pero si se eliminan los últimos eventos (o la base se restaura a un estado anterior), la cadena sigue siendo válida hasta el nuevo último. Detectar rollback exige un `chain_head` anclado fuera del workspace (export, receipt o registro externo). **El export ya incluye `chain_head`; el runtime no lo usa como ancla.**
3. **Sin permisos ni sandbox.** No hay `allow/ask/deny`; no hay ejecución de shell todavía, así que el riesgo está latente, no activo. No debe añadirse shell en 0.2 sin sandbox.

### Severidad media

4. **JCS subconjunto.** Números con la representación de `serde_json`; no se ha validado contra el corpus oficial de RFC 8785. Para los sobres actuales (enteros y strings) es suficiente; un tercero podría discrepar en flotantes.
5. **El snapshot no restaura el filesystem.** Guarda estado de rama, no archivos del workspace. La spec lo pide así en 0.1, pero un usuario puede asumir lo contrario.
6. **Import sin merge.** Solo importa a un workspace inexistente; no hay resolución de conflictos.
7. **Sin límites de payload.** Un evento o artefacto puede ser arbitrariamente grande; no hay cuota ni truncado.
8. **`branch.created` incrementa el head de la rama.** La rama nace con head en su propio evento de creación; es coherente, pero debe documentarse para no confundirlo con el `fork_event`.
9. **Sin lock de instancia única.** Dos `daemon start` concurrentes pueden arrancar dos procesos; el segundo sobrescribe `daemon.json` y el primero queda huérfano con el mismo `hli.db`. SQLite serializa las escrituras, pero el estado del daemon no es único. Mitigación: lockfile con pid o named mutex.

### Severidad baja

10. `daemon.json` no se borra si el proceso muere abruptamente; el CLI lo tolera y `daemon start` lo sobrescribe.
11. Sin `--output jsonl` real (solo `json` y `human`); la spec menciona JSONL.
12. Logs del daemon sin rotación.
13. Sin TUI ni Desktop (fuera de 0.1).

---

## 11. Guía de auditoría para Copilot

### 11.1 Qué verificar de forma independiente

1. Que `hli-cli` no dependa de `hli-store` ni de `rusqlite` (`crates/hli-cli/Cargo.toml`).
2. Que toda escritura pase por `WriteCmd` y que el actor sea el único que posee la conexión de escritura.
3. Que la secuencia se asigne dentro de `BEGIN IMMEDIATE` y no antes.
4. Que el sobre canónico incluya `previous_hash` y excluya solo `event_hash`.
5. Que los triggers append-only existan y que un `UPDATE events` falle.
6. Que `import` verifique cadena y hashes **antes** de la primera escritura.
7. Que el reinicio de las pruebas sea de proceso, no de estado en memoria.
8. Que el snapshot se cree al ramificar y quede enlazado en `branches.snapshot_id`.
9. Que la idempotencia use `NULL` para eventos sin clave.
10. Que `cargo test` pase desde limpio (`cargo clean && cargo build --workspace && cargo test`).

### 11.2 Preguntas que el auditor debería hacer

- ¿Puede un evento insertarse con `sequence` duplicada sin que la base lo rechace?
- ¿Qué ocurre si el actor muere a mitad de una transacción? ¿SQLite deja el log consistente?
- ¿Puede el CLI escribir directamente en `hli.db` con las herramientas del sistema? Sí, por diseño; ¿debería?
- ¿El hash cubre `payload` como objeto o como cadena? (Objeto; `payload_json` se guarda en JCS.)
- ¿Qué pasa si dos procesos llaman a `daemon start` a la vez? (Carrera de arranque; no hay lock de instancia única.)
- ¿El `chain_head` del export se usa como ancla en el import? (Sí, se compara.)
- ¿Se puede importar un archivo con eventos válidos pero de otro workspace? (El manifiesto fija `workspace_id`; los eventos se insertan tal cual y el `verify` del workspace lo detectaría.)
- ¿Los artefactos se verifican al importar? (Sí, por hash de archivo; no se re-hashea el contenido contra `artifacts.sha256` más allá del manifiesto.)

### 11.3 Criterios de aceptación de 0.1

- [x] `cargo build --workspace` sin errores.
- [x] `cargo test` con 16 pruebas en verde.
- [x] Reinicio de proceso con recuperación exacta.
- [x] `verify` ok con el mismo `chain_head` antes y después.
- [x] Export/import en home distinto con verificación.
- [x] Snapshot automático al ramificar.
- [x] Idempotencia demostrada.
- [x] Manifiesto manipulado rechazado.

### 11.4 Lo que este informe NO afirma

- No afirma conformidad MCP, ACP ni reproducibilidad de modelos: no hay cliente MCP ni proveedor implementados.
- No afirma que el agent loop exista.
- No afirma que el sandbox exista.
- No afirma que el JCS sea conforme al corpus oficial de RFC 8785.
- No afirma que la cadena detecte truncamiento sin un ancla externa.

---

## 12. Estado del repositorio

```
Cargo.toml                     workspace, 4 crates
crates/hli-core/src/           canon.rs, model.rs, error.rs, ids.rs, time.rs
crates/hli-store/src/          schema.rs, writer.rs, reader.rs, export.rs, lib.rs
crates/hli-daemon/src/         main.rs
crates/hli-cli/src/            main.rs
crates/hli-cli/tests/          e2e.rs
docs/adr/                      0001–0005
README.md
```

- 2 801 líneas de Rust (incluye pruebas).
- Dependencias: `rusqlite 0.37.0` (SQLite embebido), `serde 1.0.229`, `serde_json 1.0.154`, `sha2 0.10.9`, `hex 0.4`.
- 6 commits: core, store, cli/daemon, docs, fix de snapshot, informe.
- Árbol de trabajo limpio.

---

## 13. Próximo bloque recomendado

**0.2, en este orden:**

1. `run` y `ContextManifest` persistido antes de `model.requested` (invariante verificable).
2. Proveedor simulado determinista para probar el manifiesto sin red.
3. Tool Runtime con `filesystem` y `shell` **solo después** del sandbox por plataforma.
4. Receipt con `provider_class`, manifiesto, permisos y sandbox.
5. Compactación y retrieval léxico sobre FTS5.

Cada bloque con pruebas, commit y ADR, como 0.1.

---

*Fin del informe. Cualquier afirmación de este documento debe poder reproducirse con los comandos de §2; si no se reproduce, es un defecto del informe.*
