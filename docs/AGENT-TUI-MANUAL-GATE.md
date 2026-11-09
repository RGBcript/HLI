# HLI — Gate manual de Agent Mode (Windows Terminal)

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

## 2. Criterio de aceptación

- No hay pánico ni corrupción de pantalla.
- El compositor tiene el foco desde el inicio.
- No aparecen paneles de administración en Agent Mode.
- Thread y run se crean sin comandos manuales.
- El Inspector conserva estado al volver.
- El daemon sigue vivo al cerrar.

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
Pasos ejecutados:       44 / 44
Resultado:              PENDIENTE / PASA / FALLA
Observaciones:
Firma:
```

Estado actual: **PENDIENTE**. El script `scripts/agent_tui_gate.ps1` automatiza
la parte no interactiva; **no** sustituye esta firma.
