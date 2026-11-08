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

## Fallos

| Situación | Qué hace la interfaz |
|---|---|
| Daemon caído | `run.start` falla; el error aparece en el transcript |
| Provider sin configurar | usa `sim` o el último provider del workspace |
| Sin workspace vinculado | una oferta de creación; el prompt queda pendiente |
| Run `failed` | muestra el estado y el error; no reintenta solo |

## Automatización

```powershell
hli run "revisa el estado" --output json
```

Requiere un workspace vinculado al directorio actual. Sin él, falla con un
mensaje que remite a `hli`. `--async` y `--output jsonl` todavía no están
soportados.

## Lo que este flujo no hace

- No ejecuta herramientas ni permisos interactivos: el runtime actual ejecuta el
  agent loop existente, sin sandbox.
- No muestra un plan aprobable paso a paso; el plan/aprobación es el siguiente
  slice.
- No mantiene una copia durable en el cliente.
