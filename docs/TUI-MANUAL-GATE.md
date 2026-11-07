# HLI — Gate visual manual de la TUI (Windows Terminal)

**Fecha:** 2026-11-07 · **Binario:** `target\debug\hli.exe` · **Estado:** guía para ejecución manual.

## 1. Preparación (PowerShell)

```powershell
$HLI = "C:\Users\GUERRA\Documents\TEC\HLI\target\debug\hli.exe"
$env:HLI_HOME = "$env:TEMP\hli-rc-visual"   # ya tiene workspace, thread, branch, run y MCH
& $HLI daemon status
```

Si se quiere repetir desde cero:

```powershell
$env:HLI_HOME = "$env:TEMP\hli-rc-visual-2"
New-Item -ItemType Directory -Force $env:HLI_HOME | Out-Null
& $HLI daemon start
$ws = (& $HLI workspace create demo --root "$env:TEMP\hli-rc-root" | ConvertFrom-Json).id
$th = (& $HLI thread create research --workspace $ws | ConvertFrom-Json).id
$br = (& $HLI branch create main --thread $th | ConvertFrom-Json).id
& $HLI run start --workspace $ws --thread $th --branch $br --prompt "hola" --provider sim
```

Para MCH Memory:

```powershell
Set-Location "C:\Users\GUERRA\Documents\TEC\HLI"
python scripts/package_module.py
& $HLI module install dist\hli-memory-0.4.0.hlimod
& $HLI mch create memory-main --module hli-memory --domain memory --workspace $ws
& $HLI mch serve memory-main
```

## 2. Abrir la TUI

```powershell
& $HLI
```

## 3. Recorrido y registro

| # | Acción | Qué debe verse | Registro |
|---|---|---|---|
| 1 | Terminal y versión | Windows Terminal, versión y build | |
| 2 | Dimensiones | Ancho × alto aproximados | |
| 3 | Foco inicial | `▸ Navigation` con borde doble | |
| 4 | `Tab` ×4 | Collection → Inspector → Tabs → Command | |
| 5 | `1`..`6` | `[Run]`, Events, Context, Memory, Artifacts, MCH | |
| 6 | Tab `3` Context | presupuesto, user_input, memorias, slices, payload_hash | |
| 7 | Tab `4` Memory | `active` y `superseded` con autoridad | |
| 8 | Tab `5` Artifacts | id, MIME, bytes, SHA-256 | |
| 9 | Tab `6` MCH | Profile, Instance, Process, Endpoint | |
| 10 | `?` | superposición "HLI help"; `Esc` la cierra | |
| 11 | `:` → `memory search --workspace <ws> --query hola` | comando real, sin prefijo `hli` | |
| 12 | Reducir a <100 columnas | layout Medium (navegación + inspector) | |
| 13 | Reducir a <60 columnas | layout Compact (`n/5`) | |
| 14 | Reducir a <40×10 | `Terminal too small` y tamaños | |
| 15 | `NO_COLOR=1` y reabrir | sin color; `▸` y texto siguen | |
| 16 | `HLI_ASCII=1` o `TERM=dumb` | `*`, `|-`, `` `- `` en vez de `●`, `├─`, `└─` | |
| 17 | `q` | cierre normal, sin error | |
| 18 | Terminal restaurada | prompt normal, cursor visible, sin colores residuales | |
| 19 | `& $HLI daemon status` | `"running": true` | |

Comandos exactos del paso 11 y 15:

```powershell
$env:NO_COLOR = "1"
& $HLI
Remove-Item Env:NO_COLOR
```

## 4. Criterio de aceptación

- No hay pánico ni corrupción de pantalla.
- El foco siempre es visible sin depender del color.
- Las seis pestañas muestran datos o un estado vacío accionable.
- El comando del paso 11 se ejecuta y el mensaje de éxito/error aparece en la barra.
- Al salir, el terminal queda usable y el daemon sigue `running: true`.

## 5. Qué registrar si algo falla

- Paso, ancho×alto, texto esperado y texto observado.
- Si hubo pánico: mensaje completo y si el terminal quedó en raw mode.
- Si el daemon murió: `daemon status` y `daemon.log` de `$HLI_HOME`.

## 6. Resultado firmable (obligatorio antes del tag)

Rellena y firma esta sección tras ejecutar los 19 pasos:

```text
Terminal:
Versión:                hli 0.9.0-beta.1
Resoluciones probadas:
Fecha:
Commit:
Pasos ejecutados:       19 / 19
Resultado:              PENDIENTE / PASA / FALLA
Observaciones:
Firmado por:
```

Estado actual: **PENDIENTE**. El paquete Beta está listo y el gate automático
puede estar verde, pero `beta_gate.ps1` mantiene `ok=false` y `tui_manual=false`
hasta que esta sección esté firmada. No se crea el tag `v0.9.0-beta.1` sin ella.
