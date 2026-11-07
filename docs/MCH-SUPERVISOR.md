# HLI — Supervisor MCH persistente

**Estado:** parcial. Proceso persistente y reutilización implementados; health periódico, reinicio automático y autostart pendientes.

## Arquitectura

`crates/hli-daemon/src/supervisor.rs` mantiene un registro en memoria:

```
instance_id → MchHandle { pid, child, stdin, pending, alive, stderr_tail }
```

- Un proceso por instancia, lanzado una vez en `mch serve`.
- `stdin` serializado con mutex; cada petición lleva un `id` JSON-RPC único.
- Un hilo lector demultiplexa respuestas por `id` hacia `SyncSender` pendientes.
- Un hilo captura `stderr` con tope de 8 KiB.
- Al EOF, el proceso se marca `alive = false` y todas las invocaciones pendientes reciben error estructurado.
- HLI sigue siendo la fuente de verdad; el módulo no abre SQLite.

## Comandos

```powershell
hli mch serve <instance>      # lanza, initialize, deja el proceso vivo
hli mch status <instance>     # instancia en DB + pid, alive, uptime, stderr
hli mch invoke <instance> <operation> --arguments '<json>'
hli mch restart <instance>    # stop + start; nuevo PID
hli mch stop <instance>       # shutdown, espera 5 s, kill si no sale
hli mch logs <instance>       # cola de stderr
```

`mch serve` devuelve cuando el proceso respondió a `initialize`; no espera un health completo. `mch.invoke` exige `state == ready` y reutiliza el proceso.

## Persistencia

- `mch_set_state(name, state, pid)` guarda el PID efímero.
- `Store::open` marca como `stopped` y `pid = NULL` las instancias en `starting/ready/degraded/restarting/stopping`. Un reinicio del daemon no asume que el proceso anterior sigue vivo.
- **No hay autostart todavía**: las instancias no se relanzan automáticamente.

## Fallos cubiertos

| Caso | Comportamiento |
|---|---|
| El módulo muere | `alive=false`; invocaciones pendientes fallan; el daemon sigue vivo |
| El módulo no responde | `timeout` por invocación; el proceso sigue vivo |
| JSON inválido | la línea se ignora; la invocación pendiente expira por timeout |
| ID desconocido | se ignora |
| Falla `initialize` | `mch serve` devuelve error y el proceso se detiene |
| Operación no declarada | `undeclared_operation` antes de invocar |

## Pruebas

`crates/hli-cli/tests/e2e_11.rs`:

- `mch_process_is_reused_across_invocations`: mismo PID en dos invocaciones; restart cambia el PID; stop deja `stopped`.
- `mch_crash_does_not_crash_daemon`: matar el proceso marca `alive=false`, la invocación falla y el daemon sigue `running`.

## Pendiente para el gate 0.9

1. Health periódico (30 s, timeout 5 s, 2 fallos → `degraded`, 3 → restart).
2. Reinicio automático con backoff 1/2/4 s y máximo 3 en 60 s; después `failed`.
3. `autostart` por instancia al arrancar el daemon.
4. Lifecycle `capabilities` y `configure` (el Module Protocol actual solo tiene `initialize` e `invoke`).
5. Módulo de prueba configurable (fallo, cuelgue, JSON inválido, health falso, PID).
6. Pruebas restantes: degraded, restart limit, invalid JSON, shutdown limpio, kill tras timeout, autostart, invocación activa durante caída, ID desconocido.
