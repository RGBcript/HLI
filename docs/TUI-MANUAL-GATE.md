# HLI — Gate visual manual de la TUI (Windows Terminal)

**Fecha:** 2026-11-08 · **Binario:** `target\debug\hli-tui.exe` (o `hli.exe` sin argumentos)
**Estado:** guía verificada contra el código en `7a53f12`. **Sin firmar.**

Cambios respecto a la versión anterior:

- **K1 corregido** (`7a53f12`): la TUI ignora los eventos `Release`; cada pulsación
  cuenta una vez. El paso 0 queda como comprobación de regresión.
- **K2 corregido** (`7a53f12`): la barra de comandos conserva el resultado durante
  5 s (o hasta escribir o pulsar `Esc`). El paso 11 se verifica por ese mensaje.
- Pasos 4, 5 y 12–15 ajustados al comportamiento real (foco por borde doble y
  barra de pistas; `Esc` antes de `1..6`; umbrales de layout).

Evidencia automática: `docs/TUI-PTY-EVIDENCE.md` y `docs/tui-evidence/`.

---

## 0. Comprobación de regresión (teclado)

Abre la TUI y pulsa **Tab una sola vez**; después escribe `:` y `abc`.

| Observación | Significado | Acción |
|---|---|---|
| Tab avanza **un** panel y se ve `abc` | Correcto | Continuar |
| Tab avanza **dos** paneles o se ve `aabbcc` | Regresión de K1 | **Detener**. No firmar. Reportar con commit y terminal |

Registra el resultado en la sección 6.

---

## 1. Preparación (PowerShell)

`HLI_HOME` es desechable; usa una ruta propia para el gate.

```powershell
$HLI  = "C:\Users\GUERRA\Documents\TEC\Doc\Software\HLI\target\debug\hli.exe"
$env:HLI_HOME = "$env:TEMP\hli-gate-visual"
New-Item -ItemType Directory -Force $env:HLI_HOME | Out-Null

& $HLI daemon start
$ws = (& $HLI workspace create demo --root "$env:TEMP\hli-gate-root" --output json | ConvertFrom-Json).id
$th = (& $HLI thread create research --workspace $ws --output json | ConvertFrom-Json).id
$br = (& $HLI branch create main --thread $th --output json | ConvertFrom-Json).id
& $HLI run start --workspace $ws --thread $th --branch $br --prompt "hola" --provider sim | Out-Null

$old = (& $HLI memory commit --workspace $ws --type fact --content "PostgreSQL era la fuente." --scope workspace --authority legacy --output json | ConvertFrom-Json).id
& $HLI memory commit --workspace $ws --type fact --content "SQLite es la fuente." --scope workspace --supersedes $old | Out-Null

"contenido de prueba" | Set-Content "$env:TEMP\hli-gate-nota.txt"
& $HLI artifact put "$env:TEMP\hli-gate-nota.txt" --workspace $ws | Out-Null

& $HLI module install "dist\hli-memory-0.9.0-beta.1.hlimod"
& $HLI mch create memory-main --module hli-memory --domain memory --workspace $ws
& $HLI mch serve memory-main
```

`mch serve` queda en primer plano: abre una segunda ventana para la TUI o
ejecútalo en segundo plano. Si el módulo firmado no está en el trust store, la
instalación lo reporta; para el gate visual alcanza con que la instancia exista
(el estado puede ser `configured`).

**Dimensiones:** para el layout Full, la ventana debe tener **≥120 columnas y ≥16
filas**. Para los pasos 12–15 se redimensiona la misma ventana.

---

## 2. Abrir la TUI

```powershell
& $HLI
```

---

## 3. Recorrido y registro

El foco **no se valida por nombres internos**: en pantalla solo existen los
títulos de panel. Se valida por **borde doble + marca `▸`** y por la **barra de
pistas**.

| # | Acción | Qué debe verse | Registro |
|---|---|---|---|
| 1 | Terminal y versión | Windows Terminal; `hli 0.9.0-beta.1` | |
| 2 | Dimensiones | Ancho × alto reales (p. ej. 120×30) | |
| 3 | Foco inicial | Panel `Navigation` con borde doble y `▸ Navigation`; pistas `↑↓ select · → expand · Enter open · Tab panel · ? help` | |
| 4 | `Tab` ×4 | Foco: `Navigation` → `Runs` → panel derecho (`Run <id>`) → pestaña de detalle (`Run`) → **barra de comandos** (sin borde; las pistas cambian a `Enter run · Esc cancel · ↑↓ history`) | |
| 5 | `Esc`, luego `1`..`6` | `Esc` devuelve el foco a `Navigation`; cada dígito activa una pestaña (`[Run]`, `[Events]`, `[Context]`, `[Memory]`, `[Artifacts]`, `[MCH]`). En la barra los dígitos se escriben como texto: por eso va `Esc` primero | |
| 6 | Tab `3` Context | `user_input`, `payload_hash`, `memorias N · slices N · excluidos N`, `presupuesto` | |
| 7 | Tab `4` Memory | Bloques `active (N)` con `[user]` y `superseded (N)` con `superseded_at` | |
| 8 | Tab `5` Artifacts | `id  mime  N bytes  sha256:…` | |
| 9 | Tab `6` MCH | `Profile:`, `Instance:` (con estado), `Process:`, `Endpoint:` | |
| 10 | `?` y `Esc` | Superposición `HLI help`; `Esc` la cierra (también `q` o `?`) | |
| 11 | `:` → `memory commit --workspace <ws> --type fact --content gate-visual --scope workspace` → Enter | La barra muestra `✓ ok · mem_…` durante 5 s (o hasta escribir/`Esc`). Después: `:` → `workspace get noexiste` → Enter debe mostrar `× hli: not_found: …` | |
| 12 | Ventana a 90–119 columnas | Layout **Medium**: `Navigation` + inspector/detalle. Desaparece el panel `Runs` | |
| 13 | Ventana a 60–89 columnas | Layout **Compact**: línea `1/5 Navigation` y un solo panel | |
| 14 | Ventana a <60 columnas o <16 filas | Layout **Reduced**: solo el panel de detalle, **sin** línea `n/5` | |
| 15 | Ventana a <40×10 | `Terminal too small`, `Required: 40x10`, `Current: AxB` | |
| 16 | `NO_COLOR=1` y reabrir | Sin color; `▸` y borde doble siguen visibles | |
| 17 | `HLI_ASCII=1` y reabrir | Sin `●`; aparece `*`. Con dos o más ramas aparecen `` |- `` y `` `- `` | |
| 18 | `q` | Cierre normal, sin error, código 0 | |
| 19 | Terminal y daemon | Prompt normal, cursor visible, sin colores residuales; `hli daemon status` → `"running": true` | |

Comandos exactos de los pasos 16 y 17:

```powershell
$env:NO_COLOR = "1"; & $HLI; Remove-Item Env:NO_COLOR
$env:HLI_ASCII = "1"; & $HLI; Remove-Item Env:HLI_ASCII
```

Notas de observación (verificadas en consola real, ver evidencia):

- En Compact el número es **`n/5`** porque cuenta áreas de foco, no pestañas.
- El panel derecho se titula `Run <id>` o `No selection`.
- La barra de comandos no cambia de borde al enfocarse; su indicador es la barra
  de pistas y, tras ejecutar, el mensaje de feedback.

---

## 4. Criterio de aceptación

- No hay pánico ni corrupción de pantalla.
- El foco siempre es visible **sin depender del color** (borde doble + `▸`).
- Las seis pestañas muestran datos o un estado vacío accionable.
- El comando del paso 11 deja **feedback visible** (`✓` / `×`) y desaparece al
  escribir o pulsar `Esc`.
- Al salir, el terminal queda usable y el daemon sigue `"running": true`.
- El paso 0 no detecta regresión de K1.

---

## 5. Qué registrar si algo falla

- Paso, ancho×alto, texto esperado y texto observado.
- Si hubo pánico: mensaje completo y si el terminal quedó en raw mode.
- Si el daemon murió: `hli daemon status` y `daemon.log` de `HLI_HOME`.
- Si reaparece K1 o K2: anotarlo como regresión con el commit evaluado.

---

## 6. Resultado firmable (obligatorio antes del tag)

Rellena y firma esta sección tras ejecutar el paso 0 y los 19 pasos:

```text
Terminal:
Versión de Windows Terminal:
Versión HLI:            hli 0.9.0-beta.1
Commit evaluado:
Dimensiones probadas:
Fecha:
Paso 0 (teclado):       OK / REGRESIÓN K1
Pasos ejecutados:       19 / 19
Resultado:              PENDIENTE / PASA / FALLA
Observaciones:
Firma:
```

Estado actual: **PENDIENTE**. El paquete Beta está construido y los gates
automáticos están verdes, pero `beta_gate.ps1` mantiene `ok=false` y
`tui_manual=false` hasta que esta sección esté firmada. No se crea el tag
`v0.9.0-beta.1` sin ella.
