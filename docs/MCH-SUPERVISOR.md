# HLI — Supervisor MCH persistente

**Estado:** cerrado (2026-11-07). Commit inicial del bloque: `8bfdab1`.
Commit final del bloque: `feat(mch): enforce bounded automatic restarts`.

## Arquitectura

`crates/hli-daemon/src/supervisor.rs` mantiene un registro en memoria:

```
instance_id → MchHandle { pid, generation, child, stdin, pending, alive, stderr_tail }
```

- Un proceso por instancia, lanzado por `mch serve`, `mch restart`, el autostart del daemon o la recuperación automática.
- `stdin` serializado con mutex; cada petición lleva un `id` JSON-RPC único.
- Un hilo lector demultiplexa respuestas por `id` hacia `SyncSender` pendientes; una respuesta tardía de una generación anterior no encuentra `id` y se descarta.
- Un hilo captura `stderr` con tope de 8 KiB.
- Al EOF, el proceso se marca `alive = false` y las invocaciones pendientes reciben error estructurado.
- HLI sigue siendo la fuente de verdad; el módulo no abre SQLite.
- Un guard por `instance_id` serializa `start`/`stop`: autostart y `mch serve` concurrentes no lanzan dos procesos.

## Comandos

```powershell
hli mch create <instance> [--module M] [--domain D] [--workspace W] [--autostart]
hli mch set-autostart <instance> true|false
hli mch serve <instance>
hli mch status <instance>          # JSON y --output human
hli mch invoke <instance> <op> --arguments '<json>'
hli mch restart <instance>         # reinicio manual; limpia el limitador
hli mch stop <instance>
hli mch logs <instance>
```

`initialize → capabilities → configure → health` se ejecuta antes de publicar `ready`. `mch.invoke` exige `state == ready` y reutiliza el proceso.

## Persistencia

- `mch_instances.autostart` (columna aditiva, default `0`): el autostart sobrevive al reinicio del daemon.
- `Store::open` normaliza estados y PIDs de la ejecución anterior (`starting/ready/degraded/restarting/stopping → stopped`, `pid = NULL`) antes de publicar `daemon.json`.
- El limitador de reinicios vive en memoria del daemon. No se persisten timers entre reinicios del daemon: al arrancar de nuevo la instancia vuelve por autostart y el contador arranca en 0. Es una limitación consciente, documentada abajo.

## Semántica del límite

```
restart_count_in_window
  = procesos de reemplazo iniciados por recuperación automática
    dentro de restart_window
```

No cuentan:

- fallos individuales de health;
- el proceso inicial;
- reinicios manuales (`mch restart`, `mch serve`);
- el autostart inicial tras arrancar el daemon.

Política por defecto: `max_restarts = 3`, `restart_window = 60 s`.

| Reinicio automático | Resultado |
|---|---|
| 1 | permitido |
| 2 | permitido |
| 3 | permitido |
| siguiente necesidad dentro de la ventana | `state=failed`, sin proceso nuevo |

Un fallo de health solo dispara la recuperación al alcanzar `restart_after`; los fallos por debajo de ese umbral producen `degraded`, no consumen cupo.

## Política de supervisión

| Parámetro | Default | Override |
|---|---|---|
| `health_interval` | 30 s | `HLI_MCH_HEALTH_INTERVAL_MS` |
| `health_timeout` | 5 s | `HLI_MCH_HEALTH_TIMEOUT_MS` |
| `degraded_after` | 2 | `HLI_MCH_DEGRADED_AFTER` |
| `restart_after` | 3 | `HLI_MCH_RESTART_AFTER` |
| `max_restarts` | 3 | `HLI_MCH_MAX_RESTARTS` |
| `restart_window` | 60 s | `HLI_MCH_RESTART_WINDOW_MS` |
| `initial_backoff` | 1 s | `HLI_MCH_BACKOFF_MS` |

Cada proceso tiene `generation`; cada arranque la incrementa. El backoff es exponencial: `initial_backoff × 2^restart_count_in_window`.

## Estado observable

`mch status` devuelve, dentro de `process`:

| Campo | Significado |
|---|---|
| `pid`, `state`, `alive`, `generation`, `uptime_ms` | proceso actual |
| `health_failures`, `last_health_at`, `last_health_ok` | salud |
| `restart_count` | total de reemplazos automáticos desde el arranque del daemon |
| `restart_count_in_window` | reemplazos automáticos dentro de la ventana |
| `restart_window_started_at` | inicio de la ventana (null si no hay ventana activa) |
| `last_restart_at` | último reemplazo automático |
| `next_restart_at` | instante anunciado durante el backoff |
| `last_error` | `{code, message}`; `restart_limit_exceeded` al superar el límite |

Ejemplo de salida humana:

```
human-main  state=ready pid=5400 generation=1 restarts_window=0 next_restart_at=- last_error=-
```

## Ventana estable

Cuando `restart_window` expira sin un nuevo reinicio, la siguiente lectura o decisión deja:

```
restart_count_in_window = 0
restart_window_started_at = null
next_restart_at = null
```

La ventana se abre con el primer reemplazo automático y no se reinicia mientras haya actividad dentro del plazo.

## Backoff no bloqueante

- El backoff duerme solo en el hilo de health de esa instancia, en tramos de ≤ 10 ms.
- No bloquea al daemon, al Writer Actor ni a otras instancias; no mantiene transacciones SQLite.
- Durante el backoff: `state=restarting`, `next_restart_at` con instante futuro, `pid = null`.
- `mch stop` incrementa una época de cancelación: el hilo despierta, no crea proceso y deja la instancia `stopped`. Un `mch restart` manual también cancela el backoff pendiente.

## Estado failed

Al superar el límite:

```
state = failed
pid = null
alive = false
next_restart_at = null
last_error.code = restart_limit_exceeded
```

El hilo de health termina; la instancia no vuelve a arrancar automáticamente. El daemon y el resto de instancias siguen operativos. La instancia solo sale de `failed` con `mch restart`.

## Recuperación manual

`mch restart <instance>`:

- limpia `restart_count_in_window`, `restart_window_started_at` y `next_restart_at`;
- incrementa `generation`;
- ejecuta el lifecycle completo;
- termina en `ready` o `failed`.

## Aislamiento por instancia

El limitador, los timers y el backoff son por `instance_id`. `instance-a` puede quedar `failed` mientras `instance-b` sigue `ready` y el daemon responde. No existe contador global.

## Pruebas

`crates/hli-daemon/tests/mch_autostart.rs` (6):

- `mch_autostart_recovers_after_daemon_restart`
- `mch_non_autostart_remains_stopped`
- `failing_autostart_does_not_block_daemon`
- `autostart_reaches_ready_only_after_lifecycle`
- `autostart_does_not_reuse_stale_pid`
- `concurrent_serve_and_autostart_spawn_once`

`crates/hli-daemon/tests/mch_restart_limit.rs` (14):

- `mch_restart_limit_prevents_loop`
- `mch_fourth_restart_marks_failed`
- `mch_failed_instance_does_not_restart_automatically`
- `mch_stable_window_resets_restart_count`
- `mch_restart_limit_is_per_instance`
- `mch_backoff_does_not_block_daemon`
- `explicit_restart_recovers_rate_limited_instance`
- `mch_stop_cancels_pending_backoff`
- `mch_health_failure_marks_degraded`
- `mch_health_recovery_returns_ready`
- `mch_restarts_after_third_health_failure`
- `mch_restart_uses_new_pid`
- `mch_restart_increments_generation`
- `stale_health_result_is_ignored`

Cada prueba usa `HLI_HOME` independiente, políticas en milisegundos, instala el fixture por `module.install` y limpia daemon y procesos en `Drop`. Suite completa: **105 pruebas en debug y 105 en release, 0 fallos**.

## Gate externo

`scripts/mch_supervisor_gate.py` conserva las fases anteriores y añade límite, recuperación manual, autostart y memoria persistente:

```json
{
  "gate": "mch-supervisor",
  "ok": true,
  "lifecycle": true,
  "persistent_process": true,
  "same_pid": true,
  "degraded": true,
  "recovered": true,
  "restarted": true,
  "restart_limited": true,
  "autostart": true,
  "clean_closure": true
}
```

Corrida observada (2026-11-07):

| Hito | PID | Generación |
|---|---|---|
| proceso inicial | 10980 | 1 |
| reinicio automático básico | 21880 | 2 |
| reinicio manual de reset | 23476 | 3 |
| reemplazos automáticos 1–3 | 12036, 20812, 13896 | 4, 5, 6 |
| límite alcanzado | — (null) | 6, `restart_count_in_window=3` |
| recuperación manual | nuevo PID | 7 |
| autostart tras reiniciar daemon | nuevo PID | 1 (contador del daemon nuevo) |

## Limitaciones restantes

- Los timers del limitador no se persisten entre reinicios del daemon; el autostart vuelve a arrancar y el contador empieza en 0. La protección contra bucles aplica dentro de la vida del daemon.
- `generation` es un contador en memoria: se reinicia con el daemon.
- El gate es local (Windows, binarios `target/debug`); no hay CI reproducible todavía.
- Sigue pendiente la firma Ed25519 de módulos, las migraciones versionadas y el empaquetado; no forman parte de este bloque.
