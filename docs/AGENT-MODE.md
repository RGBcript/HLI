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

Implementados: `/help`, `/status`, `/inspect`, `/context`, `/memory`,
`/artifacts`, `/mch`, `/new`, `/branch`, `/clear`, `/exit`.

Pendientes en este slice: `/model`, `/provider`, `/permissions`, `/diff`,
`/resume`, `/export` (responden «no disponible en este slice»).

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
  enviar el prompt se recalcula: si el archivo cambió, se avisa
  `reference changed: … (se conserva la revisión seleccionada)`; si desapareció,
  `reference missing: …`. No se sustituye contenido en silencio.
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

## Límites de este slice

- La actividad es por polling, no streaming push del daemon.
- Sin plan aprobable ni UI de permisos; la política vigente no cambia.
- Sin UI de permisos: la política vigente no cambia y `shell.execute` sigue
  denegado.
- Sin `Ctrl+E` ni `/details`: los detalles expandibles siguen pendientes.
- `/provider`, `/model`, `/export` y el resto de la paleta avanzada: pendientes.
- El título del thread es el prompt truncado; se puede renombrar por CLI en una
  iteración posterior.
