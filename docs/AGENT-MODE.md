# HLI — Agent Mode

**Estado:** slice 1 implementado (`feat(tui)`). El resto está listado como
pendiente, no como hecho.

## Qué es

La superficie principal de HLI: abres un proyecto, ejecutas `hli`, escribes una
tarea y ves el resultado. No obliga a crear Workspace, Thread, Branch y Run a
mano; la interfaz los administra.

```
cd <proyecto>
hli
```

```
┌ HLI │ HLI/session/main │ sim/sim-1 │ completed ─────────────────
│ transcript
│ You
│   revisa el estado del repositorio
│ HLI
│   Run run_… · completed · sim/sim-1 · 45 in / 3 out
├────────────────────────────────────────────────────────────────
│ HLI/session/main │ sim/sim-1 │ completed │ ctx - │ tools 0 │ unpriced
└────────────────────────────────────────────────────────────────
```

## Vinculación por directorio

1. `hli` toma el directorio actual.
2. Busca un workspace cuyo `root_path` coincida.
3. Si existe, abre su último thread y su última rama.
4. Si no existe, muestra **una** decisión:

```
No HLI workspace is linked to this directory.
[Enter] Create workspace "<nombre-del-directorio>"
[Esc]   Cancel
```

5. El foco está siempre en el compositor.

La creación implícita, en el primer prompt:

- sin thread → `thread.create` con el prompt como título (48 caracteres);
- sin rama → `branch.create "main"`;
- siempre → `run.start` con el provider del último run del workspace, o `sim`.

## Teclas

| Tecla | Acción |
|---|---|
| `Enter` | enviar tarea (o aceptar la oferta pendiente) |
| `Shift+Enter`, `Ctrl+J` | nueva línea |
| `Ctrl+C` | cancelar run activo; si no hay, limpiar entrada; si está vacía, salir |
| `Ctrl+D` | salir con la entrada vacía |
| `Esc` | cerrar paleta/oferta; si no, limpiar entrada |
| `Up` / `Down` | historial de prompts con la entrada vacía |
| `?` | ayuda contextual |
| `/` | paleta de comandos |
| `@` | inserta el token de referencia tal cual |
| `Ctrl+O` | Inspector Mode |

Los eventos `Release` se ignoran; `Repeat` se conserva (K1).

## Slash commands

Disponibles: `/help`, `/status`, `/inspect`, `/context`, `/memory`,
`/artifacts`, `/mch`, `/branch [new <nombre>|<nombre>]`, `/new`, `/resume`,
`/details`, `/cancel`, `/model [nombre]`, `/provider [nombre]`, `/export [ruta]`,
`/clear`, `/exit`.

`/provider` lista nombre, tipo y modelo (nunca secretos) y, al cambiar, fija el
default y registra un evento `provider.handoff`. `/model` fija el modelo
solicitado para el próximo run; no hay catálogo expuesto, así que no se inventan
modelos. `/export` usa el contrato `export` del daemon. `/branch new` crea una
alternativa desde la revisión actual.

La paleta filtra por prefijo mientras escribes. No hay un segundo parser: cada
comando mapea a una operación existente del daemon o del Inspector.

## Referencias `@`

Escribir `@` abre un selector con cinco categorías: **Files**, **Artifacts**,
**Memories**, **Runs** y **Branches**.

- Fuentes: archivos del `root_path` del workspace; artifacts del workspace;
  memorias `active`; runs del thread; branches del thread.
- Búsqueda por nombre, ruta o ID. `Up`/`Down` selecciona, `Enter` inserta,
  `Esc` cierra sin tocar el compositor, `Tab`/`Shift+Tab` cambia de categoría,
  `Backspace` edita la búsqueda.
- La referencia visible es `@file:crates/…`, `@artifact:art_…`, `@memory:mem_…`,
  `@run:run_…`, `@branch:nombre`.
- Internamente se guarda `reference_type`, `stable_id`, `display_text`, hash o
  revisión y el instante de selección. El texto visible nunca es la identidad.
- **Archivos**: se guarda ruta relativa, hash SHA-256 del contenido y tamaño. Al
  enviar el prompt, el daemon revalida: si el archivo cambió o falta, **el run no
  se inicia** (`reference_changed` / `reference_missing`) antes de llamar al
  provider. La sustitución silenciosa no existe.
- **Envío estructurado**: el prompt viaja con `explicit_references` (tipo,
  `stable_id`, hash o revisión, `selected_at`, `selection_reason`), no solo con el
  token visible. El daemon las valida y materializa en `ContextManifest`; el
  `payload_hash` cubre orden, identidad, hash y contenido materializado.
- **Preflight**: antes de crear el run, la TUI valida **todas** las referencias.
  Archivos: hash actual vs. seleccionado. Memorias: estado y vigencia.
  Artifacts: existencia y hash. Runs: workspace. Branches: revisión y thread.
  Cualquier problema se agrupa en un overlay con el motivo; solo Refresh
  (`r`) está disponible para archivos cambiados, el resto exige Cancel (`c`).
  No se crea run ni se llama al provider mientras quede algo sin resolver.
- **Aislamiento**: una referencia de otro workspace o de otro thread se rechaza
  con `reference_out_of_scope` / `reference_not_found`; el provider nunca ve esa
  entidad.
- **Binarios**: un artifact binario se materializa como `metadata_only`; su
  contenido no viaja al payload como texto ni base64.
- **Orden**: el orden de las referencias forma parte del contexto y de
  `payload_hash`; export/import no lo reordena.
- **Keep no está soportado**: no existen snapshots originales persistidos. Se
  documenta como trabajo posterior.
- **Procedencia**: el receipt registra `used_explicit_reference_ids` (solo las
  incluidas en el payload) y `rejected_explicit_reference_ids`.
- **Memorias**: `valid_from`/`valid_until` se parsean como ISO-8601 y se comparan
  como instantes; una fecha inválida falla antes del provider.
- **Artifacts**: se conserva el `sha256` del artifact. **Branches**: se guarda
  `head_sequence` como revisión.

## Transcript durable

El transcript visible se reconstruye desde datos persistidos, no desde una lista
de la TUI:

- `run.prompt` → mensaje del usuario;
- `run.result.text` → respuesta del provider;
- `event.*` del run → actividad (`model.*`, `context.*`, `tool.*`);
- `run.state` + `run.receipt` → bloque terminal.

Orden: runs por `started_at`, eventos por `sequence`. Proyectar dos veces el
mismo dato produce el mismo transcript: el polling, el cambio a Inspector y el
`resume` no duplican entradas.

## Resume

- `hli resume --last`: abre el último thread del workspace vinculado al
  directorio actual.
- `hli resume`: selector de threads recientes con título, workspace, rama,
  último estado, provider/model y fecha de actividad.
- Al elegir, recupera transcript, rama, runs, receipts y artifacts **sin crear**
  otro thread.
- Si el directorio no tiene workspace, el selector muestra los recientes.

## Historial (`Ctrl+R`)

- Abre un overlay de búsqueda sobre los prompts durables del thread (derivados de
  `run.prompt`, no de una lista en memoria).
- Escribir filtra; `Ctrl+R` otra vez avanza coincidencias; `Enter` copia al
  compositor **sin enviar**; `Esc` cierra sin tocar el compositor.

## Actividad y scroll

- La actividad se muestra **contraída**: `· model.responded`. `Ctrl+E` expande o
  contrae la última actividad; `/details` alterna todas.
- La vista expandida incluye tipo, estado, inicio, duración, entrada, salida,
  `causation_id`, `correlation_id`, artifacts y error estructurado.
- El estado de expansión sobrevive al polling, al cambio Agent ↔ Inspector y al
  cambio de pestaña; no se persiste al cerrar.
- Scroll: `PageUp`/`PageDown` (12 líneas), `Ctrl+Up`/`Ctrl+Down` (3 líneas),
  `Home` al inicio, `End` vuelve al final y reactiva el seguimiento.
- Si llega actividad mientras revisas historial, la vista no se mueve y la barra
  de estado indica `N new activity · End para seguirlas`.
- El compositor y la barra de estado quedan siempre visibles.

## Resultado terminal

Al terminar un run se muestra un bloque con estado real, duración, provider,
tokens, coste (`unpriced` si no hay precio), receipt (`result_hash` +
`context_manifest`) y artifacts. Si el estado es `failed`, `cancelled`,
`timed_out` o `interrupted`, no dice «completed» y añade el error estructurado
cuando existe.

## Inspector Mode

- `hli inspect` abre el dashboard actual.
- `Ctrl+O` alterna desde Agent Mode.
- `Esc` vuelve a Agent Mode conservando compositor, selección y scroll.
- El cambio de modo no crea thread ni run.
- La fuente de verdad sigue siendo el daemon: la TUI no abre SQLite ni escribe
  el Event Store.

## Ejecución observada

- `run.start` se envía con `async: true`: el compositor queda libre mientras el
  run corre.
- Polling cada 350 ms de `run.status` y `event.list`.
- Actividad: eventos `run.*`, `model.*`, `context.*` y `tool.*` del run actual,
  sin repetir secuencias.
- Al llegar a un estado terminal se muestra el receipt: provider/model, tokens y
  coste.
- `Ctrl+C` cancela de verdad (`run.cancel`): `queued → cancelled`,
  `running → cancelling → cancelled`.
- `HLI_SIM_DELAY_MS` retrasa el proveedor `sim` (ayuda de prueba, no de
  producción) para poder observar la ejecución.

## Planificación durable y aprobación

Un run con efectos no se ejecuta al recibir el prompt: primero se planifica.

```
prompt → run queued → planning → plan durable
      → waiting_approval (si hay efectos)  → run.approve / run.reject / run.replan
      → running → estado terminal → receipt
```

- **Plan operativo, no razonamiento.** `RunPlan` guarda pasos mostrables
  (descripción, clase de operación, efecto, herramienta, objetivo), efectos
  declarados y un `content_hash`. No hay chain-of-thought ni campos privados; el
  hash cubre sólo el contenido revisado (no el estado ni la aprobación).
- **Origen.** `provider` cuando el proveedor entrega un plan estructurado;
  `runtime_generated` en el resto. El plan runtime se construye sin inventar
  archivos ni herramientas: inspección, lectura y, si el prompt lo pide,
  un paso que *solicita* permiso de escritura.
- **Efectos y política.** `read` avanza si la política efectiva lo permite;
  `write`, `network`, `external_service`, `destructive` y `unknown` exigen
  aprobación. `shell.execute` se deniega siempre en HLI 1.0. Un efecto no
  clasificable se cierra como `unknown` (nunca como `read`).
- **Instantánea de ejecución.** `execution_json` es privada, durable y hasheada:
  proveedor, modelo, endpoint, sampling, timeout, manifiesto y su hash, revisión
  de rama, política de herramientas, versiones de módulos, instancias MCH y
  handles de secreto **versionados** (jamás valores). `run.approve` ejecuta esa
  instantánea, no la configuración vigente.
- **Aprobación ligada a la revisión mostrada.** `run.approve` acepta `plan_id`;
  si no es la revisión vigente devuelve `plan_revision_changed`. Si la
  instantánea ya no es válida (provider, modelo, módulo, permiso, secreto o
  manifiesto cambiaron) se emite `plan.invalidated`, se conserva la revisión y
  se exige `run.replan`; no se reescribe nada en silencio.
- **Revisiones.** `run.replan` crea una nueva revisión (`revision`,
  `supersedes_plan_id`, `invalidation_reasons`) y una instantánea nueva con las
  condiciones actuales. El run sigue en `waiting_approval`: nunca se autoaprueba.
- **Scopes.** `once` autoriza una transición; `run` cubre el run; `workspace`
  crea un grant persistente del Permission Engine por efecto. Una aprobación no
  cubre efectos no declarados: si aparece uno, se crea una revisión y se vuelve
  a esperar.
- **Permisos del plan ≠ permisos por herramienta.** Este bloque aprueba el plan
  completo; los permisos interactivos por tool call no están implementados.
- **Persistencia y reanudación.** Plan, instantánea y estado sobreviven al
  reinicio del daemon. Un run `waiting_approval` sigue esperando; al aprobar, un
  worker relee run, manifiesto, plan y snapshot, verifica hashes y ejecuta.
- **JSONL y MCP.** El follow emite `plan.created`, `run.waiting_approval`,
  `plan.approved`, `plan.revised` y el resto de eventos del run filtrados por
  `run_id`; un segundo cliente puede aprobar mientras el primero sigue el
  stream. MCP expone `run_plan`, `run_approve`, `run_reject` y `run_replan`.
- **Receipt.** Incluye `plan_id`, `plan_hash`, `plan_origin`, `plan_revision`,
  `approval_status`, `approval_scope`, `approval_events`, `approved_effects` y
  `rejected_effects`. Un run rechazado conserva la procedencia y no tiene
  resumen de éxito.

### Aprobar desde cada superficie

| Superficie | Operación |
|---|---|
| Agent Mode | `y` once · `a` run · `w` workspace · `n` rechazar · `v` detalles · `r` replan · `Esc` cancelar |
| Inspector | mismas teclas sobre el plan seleccionado; reutiliza `run.approve/reject/replan` |
| CLI | `hli run plan/approve/reject/replan` con `--plan` y `--scope` |
| MCP | `run_plan`, `run_approve`, `run_reject`, `run_replan` (sin `execution_json`) |

### Plan approval frente a tool permission

La aprobación de plan autoriza el **conjunto declarado de efectos** (leer,
escribir, red, servicio externo, destructivo, desconocido). No autoriza una
invocación concreta.

### Permiso por invocación

Cuando una herramienta declarada requiere permiso, el run pasa a
`waiting_approval` con `approval_kind = tool` y se crea una solicitud durable:

```
Permission required
Request: perm_…
Tool: filesystem.write
Target: crates/hli-store/src/lib.rs
Effect: write
[y] once  [a] run  [w] workspace  [n] deny  [v] details  [Esc] cancel
```

- `y` autoriza **esa** invocación y se consume; `a` cubre el run; `w` crea un
  grant persistente del workspace para esa herramienta y efecto.
- Al decidir se verifica `arguments_hash`: si los argumentos cambiaron, la
  solicitud antigua no autoriza la nueva.
- `deny` no ejecuta la herramienta y cierra el run como fallido.
- `Esc` cancela la solicitud y el run.
- Los detalles muestran hash, resumen redactado, target normalizado, política y
  grants, nunca valores de secreto.
- `shell.execute` se deniega siempre, aunque haya grant.

La aprobación del plan sigue siendo previa: plan aprobado → herramienta
declarada → política → `allow | deny | ask` → solicitud → decisión → ejecución
exactamente una vez.

## Límites de este slice

- La actividad es por polling, no streaming push del daemon.
- Sin plan aprobable ni UI de permisos; la política vigente no cambia.
- Sin UI de permisos: la política vigente no cambia y `shell.execute` sigue
  denegado.
- Las referencias `@` todavía viajan como texto del prompt: su materialización
  en `ContextManifest` (`explicit_references`) es el bloque siguiente.
- `/permissions` y `/diff` no existen en la paleta: requieren información que aún
  no está disponible.
- JSONL `--follow` no está implementado; `--output jsonl` es salida terminal.
- El título del thread es el prompt truncado; se puede renombrar por CLI en una
  iteración posterior.

### Herramienta no declarada

Si el proveedor emite una herramienta o un efecto que el plan no declara, HLI
**no la ejecuta**: crea una nueva revisión del plan, emite `plan.revised` y
vuelve a `waiting_approval` con `approval_kind = plan`. La revisión anterior
queda sustituida y su aprobación no cubre la nueva. Sólo después de aprobar la
revisión se resuelve la política de la invocación concreta (`ask` → solicitud de
permiso). Plan approval y tool permission siguen siendo fronteras separadas.

### Scroll y Transcript Mode (0.9)

- El transcript se desplaza con la rueda (3 líneas, configurable con
  `HLI_WHEEL_SCROLL_SPEED`), `PgUp`/`PgDn` y `Ctrl+Home`/`Ctrl+End`.
- Subir desactiva el auto-follow; las novedades se cuentan en la barra de estado
  (`↓ N new activity`) sin mover la vista.
- La scrollbar sólo aparece si hay contenido fuera del viewport.
- `Ctrl+O` abre el Transcript Mode (búsqueda `/`, `n/N`, `{`/`}`); el Inspector
  pasa a `Ctrl+I`.
- `HLI_DISABLE_MOUSE=1` no captura el ratón: Windows Terminal recupera su
  selección nativa y el scroll por teclado sigue funcionando.

### Renderer clásico y paridad con la terminal

- `hli --classic` (o `HLI_DISABLE_ALTERNATE_SCREEN=1`) escribe la conversación
  en el scrollback nativo, sin alternate screen ni captura de ratón. Es el
  equivalente al renderer classic de Claude Code.
- `HLI_SCREEN_READER=1` usa ese mismo renderer de texto plano.
- En Transcript Mode, `[` vuelca la conversación al scrollback y `v` la exporta
  a `HLI_HOME/transcripts/`.
- `HLI_HOME/ui.toml` admite `statusline = "comando"`; el comando recibe JSON por
  stdin y su salida se añade a la barra de estado.
- Ratón: rueda, clic en la scrollbar, clic para expandir/colapsar actividad y
  arrastre para seleccionar líneas (copia al portapapeles).
