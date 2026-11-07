# HLI — Supervisor MCH persistente

**Estado:** parcial. Proceso persistente, reutilización y lifecycle completo; health periódico, reinicio automático y autostart pendientes.

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

`mch serve` ejecuta `initialize → capabilities → configure → health`; solo entonces marca `ready`. `mch.invoke` exige `state == ready` y reutiliza el proceso.

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


## Protocolo real del módulo

| Método | Declarado | Implementado (módulo) | Probado | Requerido 0.9 |
|---|---|---|---|---|
| initialize | sí | sí | sí | sí |
| capabilities | sí | sí | sí | sí |
| configure | sí | sí | sí | sí |
| invoke | sí | sí | sí | sí |
| health | sí | sí | sí | sí |
| shutdown | sí | sí | sí | sí |
| cancel | sí | sí (`{"ok":true}`) | no | no |
| checkpoint | sí | sí (`{"ok":true}`) | no | no |

El módulo `hli-memory` implementa `capabilities`, `configure`, `health`, `cancel` y `checkpoint`; la documentación anterior que afirmaba que solo existían `initialize` e `invoke` era incorrecta. No se editó para ocultar la diferencia.

Tras `capabilities`, el daemon valida que todas las operaciones declaradas en el manifest estén presentes; si falta una, cierra el proceso y devuelve `conflict`.

## Estado del gate

| Pieza | Estado |
|---|---|
| Núcleo persistente | hecho |
| Lifecycle completo | hecho |
| Health periódico | hecho; configurable por `HLI_MCH_HEALTH_INTERVAL_MS`, `HLI_MCH_HEALTH_TIMEOUT_MS` |
| Degraded | lógica implementada; falta fixture que falle health |
| Restart automático | lógica implementada (3 fallos, backoff 1/2/4, máximo 3); sin prueba de fallo |
| Autostart | pendiente |
| Módulo de prueba | creado: `crates/hli-module-fixture`, configurable por `HLI_HOME/fixture.json`; pruebas pendientes |
| Gate completo | incompleto |


## Política de supervisión

`SupervisorPolicy` con defaults operativos y overrides por entorno para tests:

| Parámetro | Default | Override |
|---|---|---|
| `health_interval` | 30 s | `HLI_MCH_HEALTH_INTERVAL_MS` |
| `health_timeout` | 5 s | `HLI_MCH_HEALTH_TIMEOUT_MS` |
| `degraded_after` | 2 | `HLI_MCH_DEGRADED_AFTER` |
| `restart_after` | 3 | `HLI_MCH_RESTART_AFTER` |
| `max_restarts` | 3 | `HLI_MCH_MAX_RESTARTS` |
| `initial_backoff` | 1 s | `HLI_MCH_BACKOFF_MS` |

Cada proceso tiene `process_generation`; cada reinicio la incrementa. `mch status` devuelve estado, PID, generación, fallos de health y último error.

## Fixture de fallos

`crates/hli-module-fixture` implementa el protocolo y modos de fallo configurables por `HLI_HOME/fixture.json`. Informa pid, health_count e invoke_count. No esta integrado aun en pruebas e2e ni en el gate externo.
