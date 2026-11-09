# HLI — Architecture Handbook

**Versión:** `0.9.0-beta.1` · **Commit:** `6f273be`

Manual de entidades. Para cada una: qué es, qué **no** es, responsabilidades,
invariantes, relaciones y un ejemplo real. Las afirmaciones se apoyan en el
esquema SQLite (`crates/hli-store/src/migrations.rs`), el despacho del daemon
(`crates/hli-daemon/src/main.rs`) y la UI (`crates/hli-tui/src/ui.rs`).

Convención de identificadores observada: `ws_`, `thr_`, `br_`, `run_`, `mem_`,
`art_` seguidos de hexadecimal. Los nombres de módulo e instancia MCH los elige
el usuario.

---

## 1. Workspace

**Qué es.** La raíz de aislamiento. Todo dato (hilos, ramas, runs, memorias,
artefactos) pertenece a un workspace y no se mezcla con otro. Tiene un `root`
opcional en disco y un `name` legible.

**Qué no es.** No es un repositorio git ni una carpeta de proyecto: es una
partición lógica del almacén. Tampoco es multi-tenant: no hay permisos por
usuario.

**Responsabilidades.**
- Agrupar y aislar datos.
- Ser la unidad de `export`/`import` y de `verify`.
- Anclar el ámbito de memorias (`scope=workspace`).

**Invariantes.**
- `workspaces.id` es clave primaria; todo lo demás referencia `workspace_id`.
- No se puede crear un hilo sin workspace existente (`require_workspace`).
- Un `export` produce un `.hliar` con los datos de **ese** workspace.

**Relaciones.** 1 workspace → N threads → N branches → N runs; N memorias; N
artefactos; N instancias MCH pueden declarar `workspace_id`.

**Ejemplo.**
```powershell
$ws = hli workspace create demo --root C:\datos\demo --output json   # ws_7811752f4bc0cedc
hli workspace list --output json
```

---

## 2. Thread

**Qué es.** Una línea de trabajo dentro de un workspace: el tema o la
conversación. Agrupa ramas.

**Qué no es.** No es una conversación con estado propio ni una sesión de modelo.
No guarda mensajes: guarda la agrupación de ramas.

**Responsabilidades.**
- Ordenar ramas bajo un tema.
- Ser el ámbito de `scope=thread` en memoria.
- Ser la unidad que la TUI muestra en navegación (`thread <title>`).

**Invariantes.**
- `threads.workspace_id` debe existir.
- Un hilo no puede tener ramas de otro workspace (se crean con `--thread`).

**Relaciones.** N threads por workspace; 1 thread → N branches.

**Ejemplo.**
```powershell
hli thread create research --workspace ws_7811752f4bc0cedc --output json  # thr_c32970fb43f23c76
```

---

## 3. Branch

**Qué es.** La trayectoria de eventos de un hilo. Es lo que hace que un run vea
unos eventos y no otros: la rama fija un `head_sequence`.

**Qué no es.** No es una rama git, no se mergea y no hay resolución de
conflictos. Tampoco es un snapshot: el snapshot es una foto derivada.

**Responsabilidades.**
- Definir el prefijo visible de la historia (`sequence <= head_sequence`).
- Soportar creación desde un evento (`--from EVENT`).
- Servir de base al `ContextManifest` y a los snapshots.

**Invariantes.**
- El contexto de un run **solo** incluye eventos de su rama y anteriores a su
  revisión (ADR-0007: se corrigió el filtro que incluía ramas hermanas).
- `head_sequence` es monótono; un snapshot guarda el estado en esa secuencia.
- Una rama pertenece a un único hilo.

**Relaciones.** 1 branch → N runs; 1 branch → N snapshots; N eventos por rama.

**Ejemplo.**
```
br_159d66f6e2164103  main  seq 9
```

---

## 4. Run

**Qué es.** Una ejecución concreta: prompt de entrada, contexto seleccionado,
llamada al proveedor y resultado. Es la unidad de trabajo y de coste.

**Qué no es.** No es una sesión interactiva multidirección (no hay turnos
encadenados dentro de un run) ni un proceso del sistema operativo: es un registro
con estado, no un proceso vivo. La cancelación y el timeout actúan sobre la
llamada en curso.

**Responsabilidades.**
- Fijar `workspace_id`, `thread_id`, `branch_id` y `branch_revision`.
- Producir un `ContextManifest` inmutable antes de llamar al modelo.
- Registrar estados: `queued`, `running`, `completed`, `failed`, `cancelled`
  (y `timeout` como fallo de plazo).
- Adjuntar el `Receipt` con uso, coste y latencia.
- Soportar aprobación (`run.approve`) y cancelación (`run.cancel`).

**Invariantes.**
- El manifiesto se persiste **antes** de la llamada (ADR-0006); si el proveedor
  falla, el run queda `failed`, nunca `queued` para siempre (bug corregido en
  0.5).
- Un run completado tiene receipt; uno fallido puede no tenerlo.
- El run no muta la rama: la historia avanza por eventos, no por el run.

**Relaciones.** N runs por rama; 1 run → 1 manifiesto → 1 receipt; el run
referencia memorias y artefactos a través del manifiesto.

**Ejemplo.**
```powershell
hli run start --workspace $ws --thread $th --branch $br --prompt "hola" --provider sim
# run_710f6c849b97926e · completed · sim/sim-1 · 45 in / 3 out
```

---

## 5. Session

**Qué es.** Un concepto operativo, no una entidad persistida: la vida de un
proceso `hli-daemon` sobre un `HLI_HOME` concreto, desde que publica
`daemon.json` hasta que se detiene.

**Qué no es.** **No existe tabla `sessions`, ni identificador de sesión, ni
historial de sesión.** Si se busca "sesión" como objeto del modelo, no está: lo
que hay es `run` + `branch` + `event log`. Esta distinción es deliberada y debe
mantenerse hasta que exista una decisión explícita (y su ADR) para introducirla.

**Responsabilidades.**
- Acotar el ciclo de vida del daemon: arranque, PID, puerto, cierre.
- Definir qué clientes pueden hablar con el almacén en ese momento.

**Invariantes.**
- Un `HLI_HOME` tiene como máximo un daemon activo (`hli.lock`).
- `daemon.json` publica `pid`, `port`, `started_at`, `version`, `transport`.
- Al reiniciar, los PIDs y estados efímeros se normalizan antes de aceptar
  comandos.

**Relaciones.** 1 sesión de daemon → N clientes (CLI, TUI, MCP) → N runs.

**Ejemplo.**
```json
{"home":"...\\hli-tui-gate\\home","pid":9372,"port":58209,
 "started_at":"2026-11-07T23:33:10.570Z","transport":"tcp-ndjson","version":"0.9.0-beta.1"}
```

---

## 6. ContextManifest

**Qué es.** El registro inmutable de **qué vio el modelo** en un run: entrada del
usuario, memorias incluidas, slices de artefactos, elementos excluidos por
presupuesto, eventos recientes y el hash del payload.

**Qué no es.** No es el prompt final en crudo ni un log de la conversación. No se
regenera: se conserva tal como se envió.

**Referencias explícitas.** `explicit_references` es una lista ordenada de
`ExplicitReferenceEntry`: tipo (`file`, `artifact`, `memory`, `run`, `branch`),
`stable_id`, `display_text`, `selected_at`, `selection_reason=user_selected`,
`validation_state`, hash o revisión, `materialization_kind`, `materialized_text`,
tokens estimados, `included` y `exclusion_reason`. El orden es el elegido por el
usuario y se conserva en el manifiesto y en el payload. Los manifiestos antiguos
sin el campo se leen como lista vacía (`serde(default)`).

**Responsabilidades.**
- Permitir auditar por qué el modelo respondió lo que respondió.
- Justificar exclusiones (`excluded` con motivo, p. ej. `budget`).
- Aportar `payload_hash` verificable.

**Invariantes.**
- Inmutable una vez creado.
- Se persiste antes de la llamada (ADR-0006).
- `payload_hash` cubre el payload canónico; cualquier cambio cambia el hash.
- `budget_tokens` y `estimated_tokens` quedan registrados.

**Relaciones.** 1 manifiesto ↔ 1 run; referencia N memorias y N slices de
artefactos; se consulta con `manifest.get` y `run.get`.

**Ejemplo (campos reales que muestra la pestaña Context).**
```
user_input    hola
payload_hash  sha256:0ddb80e0e980f0bd363fbfc5b2ea96497e19ff2a4c0667ced30b004fae3b7204
presupuesto   2048 · estimado 33
memorias 2 · slices 0 · excluidos 0
```

---

## 7. Receipt

**Qué es.** El comprobante de la llamada al proveedor: qué modelo, cuántos tokens,
cuánto costó y cuánto tardó.

**Qué no es.** No es una factura ni una garantía de precio: el coste se calcula
con la tabla local de precios (`prices.yaml`). Si no hay precio, el coste es 0.

**Responsabilidades.**
- Registrar `provider`, `model`, `usage.input_tokens`, `usage.output_tokens`,
  `usage.latency_total_ms`, `cost.total`, `cost.currency`.
- Alimentar la cabecera y el inspector de la TUI.

**Invariantes.**
- Va dentro del run, no en tabla aparte.
- Un run `completed` debe tener receipt; `failed` puede carecer de él.
- El coste nunca se recalcula hacia atrás: se guarda el calculado en su momento.

**Relaciones.** 1 receipt ↔ 1 run; depende de `Provider` y de la tabla de precios.

**Ejemplo.**
```
provider sim/sim-1 · tokens 45 in / 3 out · coste $0.000000 · latencia 0 ms
```

---

## 8. Memory

**Qué es.** Un hecho, decisión, restricción, resumen, referencia o procedimiento
con autoridad declarada, ámbito y ciclo de vida temporal. Puede superseder a otra
memoria.

**Qué no es.** No es un embedding ni una base vectorial: la búsqueda es textual
sobre SQLite. No es un chat log. No se borra silenciosamente: se supersede o se
revoca.

**Responsabilidades.**
- Guardar conocimiento con `kind` ∈ {`fact`, `decision`, `constraint`,
  `summary`, `reference`, `procedure`}.
- Acotar por `scope` ∈ {`global`, `workspace`, `thread`, `branch`}.
- Mantener historia: `status` ∈ {`active`, `superseded`, `proposed`, `revoked`}.
- Registrar autoridad (`source_authority`: `user`, `agent`, `module`, `legacy`).
- Encadenar supersesión (`supersedes`) y contradicción (`contradicts`).
- Marcar sensibilidad (`sensitivity`, hoy `internal`).

**Invariantes.**
- `content` no vacío; `kind` y `scope` validados por el writer.
- Una memoria superseded conserva `superseded_at` y no vuelve a `active` por sí
  sola.
- El manifiesto solo incluye memorias activas.
- El ámbito no cambia después de crear (se crea con `--scope`).

**Relaciones.** N memorias por workspace; N memorias referenciadas por
manifiestos; 1 memoria puede superseder a N y ser superseded por 0..1.

**Ejemplo.**
```powershell
$old = hli memory commit --workspace $ws --type fact --content "PostgreSQL era la fuente." --scope workspace
hli memory commit --workspace $ws --type fact --content "SQLite es la fuente." --scope workspace --supersedes $old
# Estado: 1 active + 1 superseded, ambas visibles en la pestaña Memory con [user]
```

---

## 9. Artifact

**Qué es.** Un contenido binario o textual almacenado por hash SHA-256, con MIME y
tamaño, del que se pueden extraer slices con motivo.

**Qué no es.** No es un archivo del proyecto ni un adjunto de chat. No se
versiona: si cambia el contenido, cambia el hash y es otro artefacto.

**Responsabilidades.**
- Guardar el contenido en `artifacts/<sha256>` dentro de `HLI_HOME`.
- Registrar `mime`, `bytes`, `sha256` en la tabla `artifacts`.
- Permitir slices (`kind`, `start`, `end`, `reason`) para ventanas de contexto.

**Invariantes.**
- El nombre en disco es el hash; dos artefactos idénticos comparten contenido.
- `artifact.put` **no** confina la ruta de origen (limitación aceptada: operación
  local del usuario).
- Un slice referencia un artefacto existente y no altera su contenido.

**Relaciones.** N artefactos por workspace; N slices por artefacto; los slices
aparecen en `ContextManifest.artifact_slices`.

**Ejemplo.**
```
art_4662d73fc83f76b6  text/plain  48 bytes  sha256:dffbeaac...
```

---

## 10. Provider

**Qué es.** El adaptador que convierte un `ChatRequest` en una respuesta de
modelo y un `Usage`. Hoy: `sim`, Ollama y OpenAI-compatible.

**Qué no es.** No es un router inteligente ni un gestor de credenciales en la
nube. No hace streaming. No garantiza precios: estima con tabla local.

**Responsabilidades.**
- `sim`: respuesta determinista sin red (usada en pruebas y gates).
- Ollama: `POST {base_url}/api/chat`.
- OpenAI-compatible: `POST {base_url}/chat/completions` con API key.
- Estimar tokens cuando el proveedor no los devuelve.
- Calcular coste con `prices.yaml` (`parse_prices_yaml`, `cost_of`).
- Resolver secretos por handle desde `HLI_HOME/secrets/<handle>`.

**Invariantes.**
- Timeout configurable por proveedor (`timeout_secs`).
- La API key nunca se serializa en el run: se referencia por handle.
- Un error del proveedor marca el run como `failed`; no lo deja pendiente.

**Relaciones.** 1 provider → N runs; el receipt depende de él; la configuración
vive en el daemon (`provider.add`, `provider.set_default`, `provider.test`).

**Ejemplo.**
```powershell
hli run start ... --provider openai --model gpt-4o-mini --base-url https://api.openai.com/v1 --secret openai
```

---

## 11. MCH

**Qué es.** Una **instancia de proceso especializado** administrada por HLI:
módulo + dominio + workspace, con lifecycle, health, endpoint y límite de
reinicios. Ejemplo: `memory-main` ejecutando `hli-memory`.

**Qué no es.** No es un contenedor ni un sandbox. No es un módulo (el módulo es el
código; la instancia es el proceso). No es un servicio de red público: habla por
stdio con el daemon y se invoca por CLI o MCP.

**Responsabilidades.**
- Arrancar el proceso del módulo con `initialize`, `capabilities`, `configure`.
- Mantener health y estado: `starting`, `ready`, `degraded`, `failed`, `stopped`.
- Supervisar: detectar caída, reiniciar con backoff, contar reinicios.
- Respetar `autostart` al arrancar el daemon.
- Exponer `mch.invoke` (CLI `hli mch invoke`, MCP `mch_invoke`).
- Guardar `process_generation` para distinguir procesos.

**Invariantes.**
- **Un solo proceso por instancia**: guard por `instance_id` (evita carreras
  entre autostart, `mch serve` y reinicios).
- Máximo **3 reinicios** dentro de la ventana; el cuarto intento deja
  `state=failed` y **no** crea proceso.
- Una instancia `failed` por límite **no** se reinicia sola: exige
  `hli mch restart <name>`, que limpia la ventana e incrementa la generación.
- Nunca se reutilizan PIDs persistidos.
- Un autostart fallido no impide que el daemon responda.

**Relaciones.** 1 instancia → 1 módulo; 1 instancia → 1 proceso; N instancias por
workspace; el estado se refleja en la pestaña MCH (`Profile`, `Instance`,
`Process`, `Endpoint`).

**Ejemplo.**
```powershell
hli mch create memory-main --module hli-memory --domain memory --workspace $ws --autostart
hli mch serve memory-main
hli mch inspect memory-main
hli mch set-autostart memory-main false
hli mch restart memory-main
```

---

## 12. Module SDK

**Qué es.** El contrato para escribir capacidades instalables: un ejecutable
empaquetado en `.hlimod`, con manifiesto, SDK declarado, operaciones y permisos,
firmable con Ed25519.

**Qué no es.** No es un plugin dinámico cargado en el proceso del daemon (es un
proceso hijo con protocolo) ni una API estable 1.0: el SDK hoy es `>=0.4 <0.5`.

**Responsabilidades.**
- Declarar `spec.sdk`, `operations` y permisos en el manifiesto.
- Responder JSON-RPC 2.0 delimitado por línea por stdio:
  `initialize`, `capabilities`, `configure`, `invoke`, `cancel`, `health`.
- Declarar efectos por operación (`read`/`write`).
- Soportar firma y verificación (`hli module sign|verify`, trust store).

**Invariantes.**
- El daemon **rechaza** un SDK incompatible (`sdk_allows`).
- Una operación no declarada no se puede invocar.
- La firma es opcional pero verificable; el estado se reporta
  (`signed-trusted`, `unsigned` + warning).
- Los checksums del paquete se verifican en la instalación.

**Relaciones.** 1 módulo → N instancias MCH; 1 módulo → 1 firma (o ninguna); el
paquete se instala en `HLI_HOME/modules/<name>/<version>`.

**Ejemplo de operaciones de `hli-memory`.**
```
memory.commit (write) · memory.get (read) · memory.search (read)
memory.update (write) · memory.forget (write) · context.build (read)
```

---

## 13. MCP

**Qué es.** La superficie de interoperabilidad para clientes externos: servidor
stdio con framing `Content-Length`, que traduce tools a comandos del daemon.

**Qué no es.** No es el protocolo de módulos (son distintos, ADR-0008). No expone
recursos ni prompts (capacidades vacías). No tiene transporte HTTP ni auth.

**Responsabilidades.**
- Responder `initialize`, `tools/list`, `tools/call`.
- Exponer 11 tools: `workspace_list`, `thread_read`, `memory_search`,
  `memory_commit`, `run_start`, `run_get`, `artifact_read`, `module_list`,
  `mch_list`, `mch_inspect`, `mch_invoke`.
- Limitar frames a 8 MiB.

**Invariantes.**
- Cada tool call lleva sus identificadores; el servidor no mantiene estado de
  protocolo.
- Tool desconocida → error `-32602`; método desconocido → `-32601`.
- Todo pasa por el daemon: el servidor MCP no abre la base.

**Relaciones.** 1 servidor MCP → 1 daemon; comparte comandos con la CLI.

**Ejemplo.**
```json
{"jsonrpc":"2.0","id":1,"method":"tools/call",
 "params":{"name":"memory_search","arguments":{"workspace_id":"ws_...","query":"fuente"}}}
```

---

## 14. TUI

**Qué es.** Un cliente de terminal del daemon: seis pestañas (Run, Events,
Context, Memory, Artifacts, MCH), cinco áreas de foco (Navigation, Collection,
Inspector, Tabs, Command) y cinco layouts según tamaño.

**Qué no es.** No es una interfaz de administración completa ni tiene streaming:
refresca por polling (runs 500 ms, eventos 1 s, listas 4 s). No escribe en la
base: ejecuta la CLI como subproceso para los comandos.

**Responsabilidades.**
- Mostrar el estado del workspace activo y del run seleccionado.
- Permitir navegar, cambiar de pestaña y ejecutar comandos (`:`).
- Degradar el layout: Full ≥120 columnas, Medium 90–119, Compact 60–89,
  Reduced <60 o altura <16, TooSmall <40×10.
- Respetar `NO_COLOR` y `HLI_ASCII`/`TERM=dumb`.

**Invariantes.**
- Un solo panel con foco visible a la vez (borde doble y marca `▸`).
- En `CommandBar` los dígitos se escriben, no cambian de pestaña.
- La salida (`q`) restaura pantalla alterna y cursor, también tras un pánico
  (hook instalado).

**Relaciones.** 1 TUI → 1 daemon; lanza `hli.exe` para comandos; lee el estado por
la API local.

**Ejemplo.**
```
╔▸ Navigation ══════════════╗┌ Runs ──────┐┌ Run run_710f6c... ─────┐
║▸ ws_7811752f...  demo     ║│▸ run_...   ││estado   completed      │
```

---

## 15. Relaciones de conjunto

```
Workspace ─┬─ Thread ── Branch ── Run ── ContextManifest
           │      │         │        └── Receipt
           │      │         └── Event (sequence, cadena SHA-256)
           │      │         └── Snapshot
           ├─ Memory (scope: global|workspace|thread|branch)
           ├─ Artifact ── ArtifactSlice
           └─ MCH instance ── Module (.hlimod) ── Firma/Trust

Clientes: CLI · TUI · MCP  →  daemon  →  store (SQLite) + providers
```

Regla de oro: **el daemon es el único que escribe**. Cualquier diseño que abra la
base desde un cliente rompe el modelo y debe rechazarse.

---

## 16. Glosario mínimo

| Término | Significado en HLI |
|---|---|
| Evento | Registro append-only con `sequence`, `type`, `payload`, hash encadenado |
| Snapshot | Foto del estado de una rama en una secuencia |
| Sobre canónico | JSON normalizado (JCS) que se hashea para la cadena |
| Slice | Ventana de un artefacto con motivo, usada en contexto |
| Handle de secreto | Nombre de archivo en `HLI_HOME/secrets/`, nunca el valor |
| Generación | Contador de procesos de una instancia MCH |
| Gate | Script externo que demuestra una propiedad con binarios reales |

---

## Planificación durable (0.9)

- `hli-agent/src/plan.rs`: construye `RunPlan` (runtime_generated o provider),
  clasifica efectos y aplica grants de workspace.
- `hli-core/src/model.rs`: `RunPlan`, `PlanStep`, `ExecSnapshot`,
  `PermissionGrant` y las tablas de transiciones de run y plan.
- `hli-store`: migración `0008_run_plans`; una fila por revisión
  (`UNIQUE(run_id, revision)`), con `execution_json` privado y hasheado.
- `hli-daemon/src/runs.rs`: `prepare` (manifiesto → plan → instantánea),
  `execute_prepared_run` (relee y verifica hashes), `approve`, `reject` y
  `replan`.
- La aprobación se liga a `plan_id`; si la instantánea dejó de ser válida se
  emite `plan.invalidated` y se exige `run.replan`.

---

## Permisos por invocación (0.10)

- `hli-agent/src/permissions.rs`: resolución deny/allow/ask, normalización de
  target, `arguments_hash`, redacción y resumen.
- `hli-store`: migración `0009_tool_permissions` (`tool_permission_requests`,
  `tool_grants`, `runs.approval_kind`).
- `hli-daemon/src/runs.rs`: creación de la solicitud antes de ejecutar,
  reanudación con verificación de hash y receipt de la decisión.
- La invocación congelada se guarda en `invocation_json` (privada). Si hubo
  redacción de claves sensibles, no se persiste y la solicitud no puede
  reanudarse: se exige replanificar.
