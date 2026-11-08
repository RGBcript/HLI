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

Slice 1: `@` inserta el token y lo conserva literal (`@art_091`, `@mem_018`,
`@branch/main`). El selector fuzzy y la resolución a ID estable desde nombre
humano quedan pendientes; no se inventa identidad.

## Inspector Mode

- `hli inspect` abre el dashboard actual.
- `Ctrl+O` alterna desde Agent Mode.
- `Esc` vuelve a Agent Mode conservando compositor, selección y scroll.
- El cambio de modo no crea thread ni run.
- La fuente de verdad sigue siendo el daemon: la TUI no abre SQLite ni escribe
  el Event Store.

## Límites de este slice

- El run es síncrono: la interfaz espera la respuesta del daemon (con `sim` es
  inmediato; con un provider real bloquea hasta terminar).
- Sin streaming de actividad ni plan/aprobación por herramienta.
- Sin UI de permisos: la política vigente no cambia y `shell.execute` sigue
  denegado.
- Sin `Ctrl+R`, `Ctrl+E`, `/details` ni expansión de actividad.
- Sin `--output jsonl` ni visibilidad de runs asíncronos.
- El título del thread es el prompt truncado; se puede renombrar por CLI en una
  iteración posterior.
