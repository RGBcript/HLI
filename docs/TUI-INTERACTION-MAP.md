# HLI — Mapa de interacción de la TUI

**Fecha:** 2026-11-07 · **Alcance:** teclado, foco y flujos.

## 1. Teclas por foco

| Tecla | Navigation | Collection | Inspector | DetailTabs | CommandBar |
|---|---|---|---|---|---|
| `Tab` | siguiente foco | siguiente | siguiente | siguiente | — |
| `Shift+Tab` | anterior | anterior | anterior | anterior | — |
| `↑` | mover selección | mover run | scroll | scroll | historial atrás |
| `↓` | mover selección | mover run | scroll | scroll | historial adelante |
| `←` | — | — | tab anterior | tab anterior | cursor (no implementado) |
| `→` | expandir (previsto) | — | tab siguiente | tab siguiente | — |
| `Enter` | abrir → Inspector | abrir → Inspector | — | — | ejecutar |
| `Esc` | limpia filtro | limpia filtro | limpia filtro | limpia filtro | cancelar |
| `:` | command bar | command bar | command bar | command bar | — |
| `/` | filtro Events | filtro Events | filtro Events | filtro Events | — |
| `?` | ayuda | ayuda | ayuda | ayuda | ayuda |
| `q` | salir | salir | salir | salir | escribe `q` |
| `1..6` | tab directa | tab directa | tab directa | tab directa | escribe dígito |

Regla: exactamente un foco. `q` no sale si el command bar tiene el foco.

## 2. Flujo: primer workspace

1. `hli` abre la TUI. Navigation muestra `No workspaces`.
2. `:` → command bar → `workspace create demo` → Enter.
3. El polling de listas (4 s) o el refresco tras el comando actualiza Navigation.
4. `Tab` hasta Collection; `↑↓` selecciona; `Enter` enfoca Inspector.

## 3. Flujo: run

1. `:` → `thread create research --workspace <ws>`.
2. `:` → `branch create main --thread <thr>`.
3. `:` → `run start --workspace <ws> --thread <thr> --branch <br> --prompt "..."`.
4. Collection muestra el run; Inspector muestra estado, provider, tokens, coste y latencia.
5. Tabs: `1` Run, `2` Events, `3` Context, `4` Memory, `5` Artifacts, `6` MCH.

## 4. Flujo: memoria

1. `:` → `memory search --workspace <ws> --query <q>`.
2. Tab `4` agrupa `active`, `superseded`, `revoked`, `proposed`.
3. La superseded muestra `superseded_at` y `source_authority`.

## 5. Flujo: MCH

1. Tab `6` muestra Profile, Instance, Process y Endpoint por separado.
2. `:` → `mch inspect memory-main` para el detalle completo.
3. `:` → `mch serve memory-main` para health.

## 6. Flujo: ayuda

1. `?` abre la superposición.
2. `Esc`, `?` o `q` la cierran.
3. La ayuda no cambia el foco subyacente.

## 7. Flujo: terminal pequeña

1. `< 90` columnas: un panel visible y selector `n/5`.
2. `< 60` o `< 16` filas: modo reducido.
3. `< 40` o `< 10` filas: `Terminal too small`, con tamaño requerido y actual.

## 8. Símbolos

| Estado | Unicode | ASCII |
|---|---|---|
| activo | `●` | `*` |
| completado | `✓` | `OK` |
| ejecutándose | `↻` | `RUN` |
| fallido | `×` | `X` |
| rama | `├─` / `└─` | `|-` / `` `- `` |

## 9. Lo que no está

- `Alt+1..6`: documentado, no implementado.
- Cursor y edición dentro del command bar: solo se añade/borra al final.
- Ratón.
- Streaming de eventos.

## Navegación (0.9)

| Contexto | Entrada | Acción |
|---|---|---|
| Agent Mode | rueda | `wheel_speed` líneas |
| Agent Mode | `PgUp`/`PgDn` | media pantalla |
| Agent Mode | `Ctrl+Home`/`Ctrl+End` | inicio / final con follow |
| Agent Mode | `Ctrl+O` | Transcript Mode |
| Agent Mode | `Ctrl+I` | Inspector |
| Transcript | `j`/`k`, `Ctrl+U/D`, `Ctrl+B/F`, `g`/`G` | desplazamiento |
| Transcript | `/`, `n`/`N` | búsqueda y coincidencias |
| Transcript | `{`/`}` | prompt anterior / siguiente |
| Transcript | `Esc`/`q`/`Ctrl+O` | volver |

## Paridad adicional (0.9)

| Contexto | Entrada | Acción |
|---|---|---|
| Transcript | `[` | volcar al scrollback nativo |
| Transcript | `v` | exportar a archivo |
| Agent Mode | clic en scrollbar | salto proporcional |
| Agent Mode | clic en actividad | expandir / colapsar |
| Agent Mode | arrastrar | seleccionar líneas y copiar |
| Arranque | `--classic` / `HLI_DISABLE_ALTERNATE_SCREEN` | renderer clásico |
| Arranque | `HLI_SCREEN_READER=1` | texto plano |

## Pestañas y modelo (0.9)

| Contexto | Entrada | Acción |
|---|---|---|
| Global | `Tab` / `Shift+Tab` | siguiente / anterior pestaña |
| Global | `Alt+1` `Alt+2` `Alt+3` | ir a Workspace / Sesión / Chat |
| Global | clic en la barra | cambiar de pestaña |
| Lista | `↑↓` | mover selección |
| Lista | `Enter` o segundo clic | entrar |
| Lista | `n` | crear workspace / sesión |
| Lista | `Esc` | volver a Chat |
| Chat | `/model` o `/provider` | selector de modelo |
| Selector | `↑↓` `Enter` `Esc` | elegir / aplicar / cerrar |
