# Anexo — matriz completa de criterios de la TUI

Este anexo conserva la matriz acumulada de 118 criterios como trazabilidad. El recorrido humano principal está en `docs/AGENT-TUI-MANUAL-GATE.md`.


**Estado:** guía para ejecución humana. **Sin firmar.**

Complementa a `docs/TUI-MANUAL-GATE.md` (Inspector). Este gate cubre la
superficie nueva: delegación desde el directorio del proyecto.

---

## 0. Preparación

```powershell
$HLI = "C:\Users\GUERRA\Documents\TEC\Doc\Software\HLI\target\debug\hli.exe"
$env:HLI_HOME = "$env:TEMP\hli-agent-gate"
New-Item -ItemType Directory -Force $env:HLI_HOME | Out-Null
& $HLI daemon start
```

Entra en un directorio **sin** workspace vinculado:

```powershell
cd $env:TEMP
New-Item -ItemType Directory -Force demo-project | Out-Null
cd demo-project
& $HLI
```

---

## 1. Recorrido

| # | Acción | Qué debe verse |
|---|---|---|
| 1 | `hli` en un directorio sin vínculo | `No HLI workspace is linked to this directory.` con `[Enter] Create workspace "<dir>"` |
| 2 | `Enter` | `Workspace created: ws_…` y el compositor con foco |
| 3 | Escribir una tarea y `Enter` | Se crea thread y rama; aparece `Run run_… · completed` |
| 4 | Segundo prompt | Mismo thread (el encabezado no cambia de título) |
| 5 | `Ctrl+O` | Inspector: `Navigation`, `Runs`, `Context`, `Memory`, `Artifacts`, `MCH` |
| 6 | `Esc` | Vuelve a Agent Mode con el texto del compositor intacto |
| 7 | `/` | Paleta de comandos; `/inspect` abre Inspector; `/clear` limpia el transcript |
| 8 | `/branch` | Oferta `Create branch "alternative-2"`; `Enter` la crea |
| 9 | `@` | Inserta el token literal (`@art_…`, `@mem_…`) sin romper el prompt |
| 10 | `Up` con compositor vacío | Recupera el prompt anterior |
| 11 | `Ctrl+C` con run terminado | Limpia la entrada, no cierra |
| 12 | `Ctrl+D` con entrada vacía | Cierra el cliente |
| 13 | `hli inspect` | Abre directamente el Inspector |
| 14 | `hli run "tarea" --output json` desde el proyecto | JSON con `state` del run |
| 15 | `hli daemon status` | `"running": true` |
| 16 | Redimensionar a 99, 59 y 39 columnas | 59: transcript simplificado; 39: `Terminal too small` |
| 17 | `NO_COLOR=1` | Sin color; el foco y los marcadores siguen visibles |
| 18 | `HLI_ASCII=1` | Sin `●`; aparece `*` |
| 19 | Cerrar y `hli` de nuevo | Vincula el workspace ya creado y abre el último thread |

---

## 1b. Ejecución observada

Con `$env:HLI_SIM_DELAY_MS = "2500"` antes de abrir `hli`:

| # | Acción | Qué debe verse |
|---|---|---|
| 20 | Enviar una tarea | `run running` en el transcript y en la barra; la actividad aparece sin bloquear el compositor |
| 21 | Esperar | `completed · sim/sim-1 · N in / M out · $0.000000` |
| 22 | Otra tarea y `Ctrl+C` | `run cancelling` y después `cancelled`; nunca `completed` |

Sin retardo, `HLI_SIM_DELAY_MS` no se define y el run puede completarse antes del
primer poll: es correcto.

---

## 1c. Interacción avanzada (Slice 3a)

| # | Acción | Qué debe verse |
|---|---|---|
| 23 | `hli resume` en el proyecto | Selector con los threads recientes (título, workspace, rama, estado, modelo) |
| 24 | `Enter` sobre uno | `Resumed: …` y el transcript reconstruido, sin crear otro thread |
| 25 | `Ctrl+R` y escribir parte de un prompt | Overlay de historial filtrado; `Enter` copia al compositor sin enviar |
| 26 | Tras un run | Bloque terminal con estado, proveedor, tokens, coste, receipt y artifacts |
| 27 | Cambiar a Inspector y volver | El transcript no duplica entradas |

---

## 1d. Referencias `@` (Slice 3b)

| # | Acción | Qué debe verse |
|---|---|---|
| 28 | `@` en el compositor | Selector con categoría `Files` y la lista de archivos del proyecto |
| 29 | Escribir parte de un archivo y `Enter` | Se inserta `@file:ruta/relativa` y el archivo no se copia al prompt |
| 30 | `Tab` dos veces y `Enter` | Se inserta `@memory:mem_…` (o `@artifact:art_…` si no hay memorias) |
| 31 | `Esc` con el selector abierto | El compositor queda como estaba antes de `@` |
| 32 | Modificar un archivo ya referenciado y enviar | Aviso `reference changed: …`; la referencia no se sustituye |

---

## 1e. Actividad, scroll y comandos (Slice 3b, interacción)

| # | Acción | Qué debe verse |
|---|---|---|
| 33 | `Ctrl+E` sobre actividad | Se expanden tipo, estado, duración, entrada/salida y error |
| 34 | `Ctrl+E` otra vez | Vuelve a contraerse |
| 35 | `/details` | Todas las actividades cambian de estado |
| 36 | `PageUp` / `PageDown` | El transcript se desplaza sin mover compositor ni barra |
| 37 | Actividad nueva con la vista arriba | No se mueve; aparece `N new activity` |
| 38 | `End` | Vuelve al final y desaparece el indicador |
| 39 | `/provider` | Lista nombre, tipo y modelo; sin secretos |
| 40 | `/provider <nombre>` | Cambia el provider y registra `provider.handoff` |
| 41 | `/model <nombre>` | Fija el modelo del próximo run |
| 42 | `/export` | Produce un `.hliar` y muestra la ruta |
| 43 | `/branch` y `/branch new` | Lista ramas y crea una alternativa |
| 44 | `/cancel` con un run activo | Pasa a `cancelling` y luego terminal |

---

## 1f. Referencias explícitas: preflight y procedencia (Slice 3b)

| # | Acción | Qué debe verse |
|---|---|---|
| 45 | Seleccionar `@file` sobre `Cargo.toml` | El compositor incluye `@file:Cargo.toml` |
| 46 | Modificar `Cargo.toml` fuera de la TUI | — |
| 47 | Enviar el prompt | Overlay `references`: `1 reference(s) require attention` |
| 48 | Comprobar el overlay | `changed`, `Selected:` y `Current:` con hashes distintos |
| 49 | `r` (Refresh) | La referencia se actualiza y el run inicia |
| 50 | Repetir con `c` (Cancel) | No se crea run; el compositor conserva el texto |
| 51 | `@memory` activa | La referencia aparece en Context y en el manifiesto |
| 52 | Ver receipt | `used_explicit_reference_ids` contiene la memoria y el archivo |
| 53 | `resume` | La conversación conserva el transcript y el receipt |
| 54 | Referencia `missing` | Solo Cancel; no se ofrece Keep |
| 55 | Memoria superseded | El run se rechaza antes del provider |
| 56 | `hli export` y `import` | Manifiesto y receipt conservan referencias y orden |

---

## 1g. Invariantes de referencias (v1.1)

| # | Acción | Qué debe verse |
|---|---|---|
| 57 | Seleccionar `@memory` y supersederla antes de enviar | — |
| 58 | Enviar el prompt | Preflight con `superseded` para esa memoria |
| 59 | `c` Cancelar | No se crea run y el compositor conserva el texto |
| 60 | Seleccionar `@branch` | Token con la rama y su revisión |
| 61 | Provocar un cambio de revisión (nuevo run/evento) | — |
| 62 | Enviar | Preflight `revision mismatch`, solo Cancel |
| 63 | Archivo modificado + memoria superseded a la vez | — |
| 64 | Enviar | Ambos aparecen agrupados en el mismo overlay |
| 65 | `c` Cancelar | Compositor y tokens intactos |
| 66 | Resolver y ejecutar un run válido | Run completado |
| 67 | Ver el receipt | `used_explicit_reference_ids` con las referencias usadas |
| 68 | `resume` | El transcript y el receipt conservan la procedencia |

---

## 2. Criterio de aceptación

- No hay pánico ni corrupción de pantalla.
- El compositor tiene el foco desde el inicio.
- No aparecen paneles de administración en Agent Mode.
- Thread y run se crean sin comandos manuales.
- El Inspector conserva estado al volver.
- El daemon sigue vivo al cerrar.


---

## 4. Planificación y aprobación durable (pasos 69–96)

Se ejecutan después de los 68 anteriores, sobre un workspace limpio. El gate
automático (`scripts/planning_approval_gate.py`) cubre la parte no interactiva;
esta tabla verifica la experiencia real en Windows Terminal.

| # | Acción | Qué debe verse | Registro |
|---|---|---|---|
| 69 | Enviar una tarea read-only | `↻ Preparing plan` y avance sin aprobación | |
| 70 | Ver el Plan | origen `runtime-generated` (o `provider`) y pasos | |
| 71 | Confirmar avance según policy | `completed` sin overlay | |
| 72 | Enviar una tarea con `write` | overlay `Plan requires approval` | |
| 73 | Estado | `waiting_approval`, sin fichero escrito | |
| 74 | `v` detalles | hashes, provider, manifiesto, scope, módulos, MCH | |
| 75 | Confirmar hashes | `plan hash` y `snapshot hash` con prefijo `sha256:` | |
| 76 | `y` aprobar once | el overlay se cierra y el run ejecuta | |
| 77 | Confirmar ejecución | fichero escrito y estado terminal | |
| 78 | Receipt | `plan_id`, revisión, `approval_scope=once`, efectos | |
| 79 | Otra tarea con `write` | vuelve a esperar | |
| 80 | `n` + razón | entrada de razón y rechazo | |
| 81 | Confirmar no ejecución | sin fichero; estado `cancelled` | |
| 82 | `Ctrl+O` Inspector | sección Plan read-only del run | |
| 83 | `y`/`n` desde Inspector | misma operación que el overlay | |
| 84 | Volver a Agent Mode | el overlay sigue activo | |
| 85 | Dejar un Run waiting | `waiting_approval` | |
| 86 | Reiniciar el daemon | `daemon stop` y `start` | |
| 87 | `resume` | el thread recupera el plan una sola vez | |
| 88 | Confirmar Plan recuperado | misma revisión y hash | |
| 89 | Aprobar desde otra CLI | `hli run approve … --plan …` | |
| 90 | Overlay | se cierra sin duplicar eventos | |
| 91 | Cambiar modelo/provider | `hli provider add …` antes de aprobar | |
| 92 | Aprobar | `plan_stale`: el overlay muestra `Plan invalidated` | |
| 93 | `r` Replan | nueva revisión y snapshot | |
| 94 | Confirmar `revision + 1` | `Revision: N+1`, `supersedes` visible | |
| 95 | Aprobar la nueva revisión | exige decisión nueva; no se hereda | |
| 96 | Receipt y JSONL | `superseded_plan_ids`, `invalidation_events` y eventos plan.* | |

### Resultado esperado

- Ningún paso aprueba automáticamente.
- Un plan invalidado nunca acepta `y`/`a`/`w`.
- El replan no reutiliza la aprobación anterior.
- Los valores de secreto no aparecen en detalles ni en el receipt.


---

## 5. Permisos por invocación (pasos 97–118)

Se ejecutan después del bloque anterior. Cada paso usa un workspace limpio salvo
donde se indique.

| # | Acción | Qué debe verse | Registro |
|---|---|---|---|
| 97 | Aprobar un Plan con `write` | el plan pasa a aprobado | |
| 98 | Estado | overlay `Permission required` con tool y target | |
| 99 | `v` detalles | `arguments_hash`, resumen, política y grants | |
| 100 | Confirmar | target normalizado y hash con prefijo `sha256:` | |
| 101 | `y` allow once | el overlay se cierra y la tool ejecuta | |
| 102 | Confirmar ejecución única | un solo `tool.completed` | |
| 103 | Iniciar otro run | plan nuevo, permiso nuevo | |
| 104 | `a` allow for run | ejecuta sin pedir de nuevo en ese run | |
| 105 | Confirmar | no aparece una segunda pregunta dentro del run | |
| 106 | Confirmar expiración | un run nuevo vuelve a preguntar | |
| 107 | `w` allow in workspace | tool ejecutada | |
| 108 | Nuevo run en el mismo workspace | no pregunta por la misma herramienta | |
| 109 | `n` denegar | run falla, tool no ejecuta | |
| 110 | Confirmar no ejecución | sin fichero, evento `tool.permission_denied` | |
| 111 | Cerrar la TUI con permiso pendiente | el daemon conserva la solicitud | |
| 112 | `resume` | overlay recuperado | |
| 113 | Confirmar | mismo `request_id`, sin duplicar | |
| 114 | Aprobar desde otra CLI | `hli permission approve <id> --scope once` | |
| 115 | Overlay | se cierra solo | |
| 116 | Receipt | `permission_decisions`, hash y scope | |
| 117 | JSONL | `tool.permission_requested/approved/execution_resumed` | |
| 118 | Inspector | solicitud, decisión, actor y timestamps | |

### Resultado esperado

- El plan aprobado no autoriza la invocación: siempre se resuelve la política.
- `once` se consume; `run` expira con el run; `workspace` persiste.
- `shell.execute` nunca se ejecuta.
- Ningún valor de secreto aparece en overlay, Inspector, CLI, MCP ni receipt.

---

## 3. Resultado firmable

```text
Terminal:
Versión de Windows Terminal:
Versión HLI:            hli 0.9.0-beta.1
Commit evaluado:
Dimensiones probadas:
Fecha:
Paso 0 (teclado):       OK / REGRESIÓN
Pasos ejecutados:       68 + 28 (planificación) + 22 (permisos)
Resultado:              PENDIENTE / PASA / FALLA
Observaciones:
Firma:
```

Estado actual: **PENDIENTE**. El script `scripts/agent_tui_gate.ps1` automatiza
la parte no interactiva; **no** sustituye esta firma.

### Nota sobre herramientas no declaradas (97–118)

Si el proveedor emite una herramienta que el plan no declara, la TUI muestra
`Plan requires approval` con una revisión nueva (`Revision: N+1`) antes de
cualquier solicitud de permiso. El archivo no existe hasta aprobar la revisión y,
después, el permiso concreto. El proveedor `fixture` (feature
`fixture-provider`, fuera de `dist`) reproduce ese caso para el gate automático.
