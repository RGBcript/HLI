# HLI — Flujo de delegación

Cómo se convierte una frase en un run durable sin pedirle al usuario que
administre el árbol de entidades.

## Secuencia

```
directorio actual
   │  hli
   ▼
workspace vinculado (o una confirmación)
   │  primer prompt
   ▼
thread (si no hay)  →  branch main (si no hay)  →  run.start
   │
   ▼
run completado  →  receipt  →  transcript
   │  segundo prompt
   ▼
mismo thread, misma rama →  nuevo run
```

## Ejemplo real (provider `sim`)

```
You
revisa el estado del repositorio

HLI
Run run_d2330d4abf40ec9a · completed · sim/sim-1 · 45 in / 3 out
```

Lo que ocurrió por debajo, sin que el usuario lo pidiera:

1. `workspace.list` y comparación de `root_path` con el directorio actual.
2. `thread.create` con el prompt como título.
3. `branch.create "main"`.
4. `run.start` con `workspace_id`, `thread_id`, `branch_id`, `prompt` y el
   provider del último run del workspace (o `sim`).
5. El run queda en el Event Store con su `ContextManifest` y su `Receipt`.

## Reglas

- **Una sola confirmación**: crear workspace es la única decisión explícita.
- **Continuidad**: el segundo prompt reutiliza thread y rama; no crea otros.
- **Durabilidad**: si el cliente se cierra, el run y el evento quedan.
- **Auditoría**: el run tiene `payload_hash`, receipt y eventos; el transcript
  del cliente es cosmético y no es fuente de verdad.
- **Alternativas**: `/branch` ofrece crear `alternative-N` desde la revisión
  actual. La rama es durable; el usuario no gestiona el árbol.

## Reanudar

`hli resume` lista los threads recientes y reconstruye su transcript desde runs y
eventos. `hli resume --last` va directo al último thread del workspace vinculado.
No se crea nada nuevo: el daemon sigue siendo la fuente de verdad.

## Fallos

| Situación | Qué hace la interfaz |
|---|---|
| Daemon caído | `run.start` falla; el error aparece en el transcript |
| Provider sin configurar | usa `sim` o el último provider del workspace |
| Sin workspace vinculado | una oferta de creación; el prompt queda pendiente |
| Run `failed` | muestra el estado y el error; no reintenta solo |

## Automatización

```powershell
hli run "revisa el estado" --output json      # objeto completo
hli run "revisa el estado" --output jsonl     # una línea JSON
hli run "revisa el estado" --async            # devuelve run_id y sale
```

Requiere un workspace vinculado al directorio actual. Sin él, falla con un
mensaje que remite a `hli`.

La ejecución observada usa el mismo camino: `run.start` con `async: true`,
polling de `run.status` y actividad por `event.list`.

### Streaming JSONL

```powershell
hli run "tarea" --async --output jsonl --follow
```

Emite una línea JSON por evento persistido (`{"type":"event",...}`) en orden de
`sequence`, desde el primero del run, sin duplicados ni ANSI. Al llegar a un
estado terminal emite una proyección `{"type":"receipt","source":"run.status",...}`
que **no** es un evento durable. Código de salida 0 solo si el run terminó
`completed`.

`Ctrl+C` solicita `run.cancel` y espera el estado terminal; una segunda pulsación
(o `--cancel-timeout-secs`, 10 s por defecto) cierra el cliente sin matar el
daemon. `--timeout-secs` sigue siendo el timeout del provider y no se ve afectado
por la espera de cancelación. Si el consumidor cierra la tubería, el cliente
termina sin cancelar el run.

Cada evento de un run lleva `run_id` en el payload y en el sobre
(`run_id`/`correlation_id`), así que dos runs concurrentes en la misma rama
producen streams separados.
`--output jsonl` sin `--follow` sigue siendo una única línea terminal.

## Lo que este flujo no hace

- No ejecuta herramientas ni permisos interactivos: el runtime actual ejecuta el
  agent loop existente, sin sandbox.
- No muestra un plan aprobable paso a paso; el plan/aprobación es el siguiente
  slice.
- No mantiene una copia durable en el cliente.

---

## Plan y aprobación

El flujo ya no ejecuta al recibir el prompt:

```
prompt → run durable → ContextManifest → RunPlan
      → waiting_approval (si hay efectos)
      → approve | reject | replan
      → ejecución reanudable → Receipt
```

La aprobación cubre el conjunto declarado de efectos. Si durante la ejecución
aparece una herramienta no declarada, se crea una revisión y el run vuelve a
esperar. Los permisos interactivos por tool call son el bloque siguiente.

---

## Permisos por invocación

```
plan aprobado
→ tool declarada
→ política (deny | ask | allow)
→ ask: ToolPermissionRequest durable
→ waiting_approval (approval_kind = tool)
→ decisión (once | run | workspace)
→ verificación de arguments_hash
→ ejecución exactamente una vez
```

Una herramienta no declarada obliga a revisar el plan antes de pedir permiso:
no se autoriza por la vía del permiso concreto.

### Revisión por herramienta no declarada

```
running → provider emite tool no declarada
        → no se ejecuta
        → plan.revised (revision + 1, supersedes)
        → waiting_approval (approval_kind = plan)
        → aprobación de la revisión
        → política ask → ToolPermissionRequest
        → aprobación del permiso
        → ejecución exactamente una vez
```

La aprobación de la revisión anterior nunca cubre la nueva.
