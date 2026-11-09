# HLI — Recorrido humano de Agent Mode

**Binario:** `target\debug\hli.exe` (debug) o el paquete instalado
**Estado:** pendiente de firma · **No automatizable**

Este documento es el recorrido humano principal. La matriz completa de 118
criterios vive en `docs/AGENT-TUI-MANUAL-GATE-ANNEX.md` como trazabilidad. El
gate automático (`scripts/agent_tui_gate.ps1`) cubre la parte no interactiva y
nunca sustituye esta firma.

---

## Parte A — Datos del entorno

```powershell
$HLI = "C:\Users\GUERRA\Documents\TEC\Doc\Software\HLI\target\debug\hli.exe"
$env:HLI_HOME = "$env:TEMP\hli-manual-gate"
New-Item -ItemType Directory -Force $env:HLI_HOME | Out-Null
$PROJECT = "$env:TEMP\hli-manual-project"
New-Item -ItemType Directory -Force $PROJECT | Out-Null
Set-Location $PROJECT
```

Terminal, versión de Windows, dimensiones y fecha se registran en la Parte E.

---

## Parte B — Recorrido principal

Ejecutar en orden, marcando cada paso.

| # | Acción | Resultado esperado | OK |
|---|---|---|---|
| 1 | `& $HLI` desde un directorio sin workspace | Agent Mode pide crear workspace | |
| 2 | `Enter` para crearlo | aparece el workspace vinculado | |
| 3 | Enviar `resume el directorio` | run read-only hasta completar | |
| 4 | Observar la actividad | `↻ Preparing plan` y ejecución sin overlay | |
| 5 | Leer la respuesta | eventos en orden causal (`run.queued` → plan → ejecución → `run.completed`) y el resumen terminal **después** del evento terminal | |
| 6 | Enviar `tool:write manual.txt hola` | el compositor queda vacío; plan en `waiting_approval` | |
| 7 | Revisar el overlay | pasos, efectos y `write · approval required` | |
| 8 | `y` (once) | el plan se aprueba y el run continúa | |
| 9 | Ver el resultado | overlay `Permission required` con tool y target | |
| 10 | `y` (once) | la tool ejecuta una sola vez | |
| 11 | Confirmar | `manual.txt` contiene `hola` y el run termina | |
| 12 | Enviar otra tarea con write y pulsar `n` | rechazo con razón; no se escribe nada | |
| 13 | Confirmar | run `cancelled` o `failed`, archivo inexistente | |
| 14 | Escribir `@` en el compositor y elegir un archivo | referencia insertada | |
| 15 | Modificar ese archivo en otro terminal y `Enter` | preflight avisa del cambio | |
| 16 | Elegir `Refresh` o `Cancel` | el envío se detiene o actualiza sin perder la referencia | |
| 17 | `Ctrl+O` | Inspector con Run, Events, Context, Memory, Artifacts, MCH | |
| 18 | Revisar Plan y Permission | hashes, revisión, estado y decisión | |
| 19 | Volver a Agent Mode con `Ctrl+O` | overlay y compositor intactos | |
| 20 | Enviar una tarea larga (`--async` desde la TUI) | actividad en curso, compositor libre | |
| 21 | `Ctrl+C` | cancelación real y estado `cancelled` | |
| 22 | Salir con `q` | la terminal se restaura | |
| 23 | `& $HLI daemon status` | daemon activo | |
| 24 | `& $HLI resume --last` | recupera el thread anterior | |
| 25 | Confirmar | transcript reconstruido sin duplicar plan ni decisión | |
| 26 | `$env:NO_COLOR=1; & $HLI` | sin color, mismo contenido | |
| 27 | `$env:HLI_ASCII=1; & $HLI` | glifos ASCII, misma funcionalidad | |
| 28 | Redimensionar a Full, Medium, Compact, Reduced y TooSmall | el layout se adapta sin perder estado | |
| 29 | `q` | salida limpia | |
| 30 | Comprobar la terminal | cursor visible, pantalla y raw mode restaurados | |

---

### Compositor

Tras un envío aceptado, el compositor queda vacío. Si `run.start` falla antes de
crear el run, el texto se conserva para reintentar. `resume` nunca reinyecta el
último prompt en el compositor.


### Scroll y Transcript Mode

| # | Acción | Resultado esperado | OK |
|---|---|---|---|
| 1 | Abrir una conversación con más de 100 líneas | transcript con scrollbar | |
| 2 | Rueda física hacia arriba | la vista sube 3 líneas por muesca | |
| 3 | Confirmar | scrollbar proporcional y visible | |
| 4 | Generar actividad nueva mientras se revisa | el contenido llega sin mover la vista | |
| 5 | Confirmar | la posición histórica se mantiene | |
| 6 | Confirmar indicador | `↓ N new activity · Ctrl+End` | |
| 7 | Bajar con la rueda hasta el final | auto-follow reactivado | |
| 8 | Confirmar | el indicador desaparece | |
| 9 | `PgUp` / `PgDn` | media pantalla | |
| 10 | `Ctrl+Home` / `Ctrl+End` | inicio / final | |
| 11 | `Ctrl+O` | Transcript Mode a pantalla completa | |
| 12 | `/` y una cadena | coincidencias resaltadas | |
| 13 | `n` / `N` | recorre coincidencias | |
| 14 | `{` / `}` | prompt anterior / siguiente | |
| 15 | `q` o `Esc` | vuelve a Agent Mode | |
| 16 | Confirmar | compositor intacto | |
| 17 | Salir de HLI | sin residuos | |
| 18 | Confirmar | cursor visible, pantalla y raw mode restaurados, ratón liberado | |
| 19 | Repetir con `HLI_DISABLE_MOUSE=1` | sin captura de ratón | |
| 20 | Confirmar | selección nativa y navegación por teclado | |

## Parte C — Layouts y accesibilidad

- Full, Medium, Compact y Reduced mantienen foco, overlay y compositor.
- TooSmall muestra el aviso y no rompe el estado.
- `NO_COLOR` y `HLI_ASCII` no cambian el comportamiento, sólo la presentación.
- El texto del overlay cabe sin cortar el target ni los controles.

---

## Parte D — Restauración de terminal

- Cursor visible tras salir o tras un error.
- Sin secuencias de alternate screen residuales.
- `stty`/modo raw no queda activo.
- El daemon sigue vivo después de cerrar la TUI.
- Sin procesos `hli-*` huérfanos.

---

## Parte E — Resultado y firma

```text
Terminal:
Versión de Windows:
Versión HLI:
Commit evaluado:
Fecha:
Dimensiones:
Resultado: PASS | FAIL
Observaciones:
Confirmación del usuario:
```

Sólo una firma con `PASS` permite a `scripts/beta_gate.ps1` considerar cerrado
el gate humano. El gate automático no escribe este bloque.
