# HLI — Runs asíncronos

**Estado:** implementado en `d026f67`+ (`e2e_10`). Sin tag.

## Comandos

```powershell
hli run start ... [--async] [--sim-delay-ms N] [--timeout-secs N]
hli run status <run_id>
hli run wait <run_id> [--timeout-secs N]
hli run cancel <run_id>
hli run list --workspace <id> [--state <estado>]
```

`hli run start` sin `--async` conserva el comportamiento síncrono de 0.1–0.5.
`--async` persiste `run.created`, lanza un worker y devuelve el `run_id` con estado `queued` o `running`.

## Estados y transiciones

```
queued → running | waiting_approval | paused | completed | cancelled | failed | timed_out | interrupted
running → waiting_approval | paused | cancelling | completed | failed | timed_out | interrupted
cancelling → cancelled | failed
waiting_approval → running | paused | completed | failed | timed_out | cancelled | interrupted
paused → running | completed | failed | timed_out | cancelled | interrupted
terminal → (ninguna)
```

La tabla es `hli_core::model::run_transition_allowed`. `finish_run` y `set_run_state` la aplican; una transición inválida devuelve `conflict`.

## Cancelación

- `queued`: pasa a `cancelled` inmediatamente.
- `running`: pasa a `cancelling`, se activa el `AtomicBool` del worker y este termina en `cancelled`.
- Terminal: idempotente, devuelve `idempotent: true`.
- El worker nunca mata el daemon para cancelar.

El receipt se genera con el estado terminal real y se persiste. Un run cancelado no emite `run.completed`.

## Timeout

El worker mide el tiempo desde `running`; si supera `timeout_secs`, el estado final es `timed_out`. Un error de red con `timeout` en el mensaje también termina en `timed_out`; el resto, en `failed`.

`--sim-delay-ms` es una ayuda de prueba para el proveedor `sim`; no existe en producción.

## Recuperación

Al abrir el store, `Store::open` ejecuta:

```sql
UPDATE runs SET state = 'interrupted', ended_at = ?
WHERE state IN ('queued','running','cancelling','waiting_approval')
```

Un reinicio nunca interpreta un run incompleto como completado. Los terminales se conservan.

**Limitación:** `queued` también pasa a `interrupted`; no se reencola automáticamente porque no hay una cola persistente con provider/manifest reconstruibles. La reanudación con checkpoints idempotentes queda para después de 1.0.

## Pruebas

`crates/hli-cli/tests/e2e_10.rs`:

- `async_start_returns_persisted_run`
- `cancel_running_run_and_restart`
- `timeout_marks_run_timed_out`
- `daemon_restart_marks_running_interrupted`

## Limitaciones de este bloque

- El tool loop no está soportado en runs asíncronos: si el proveedor devuelve `tool_call`, el run termina `failed` con error explícito.
- MCP aún no expone `run_cancel`; sí `run_start`, `run_get` y `run_list`.
- La TUI muestra los estados y permite cancelar desde la command bar, pero no se añadió una vista específica.
- `waiting_approval` se conserva en la tabla, pero el flujo de aprobación asíncrono no se probó.
