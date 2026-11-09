# Claude Code en terminal: qué copiar y qué no

**Fuentes oficiales consultadas (2026-11-09):**
`https://code.claude.com/docs/en/fullscreen`,
`https://code.claude.com/docs/en/interactive-mode`,
`https://code.claude.com/docs/en/keybindings`.

Documento de investigación. No reproduce textos ni branding; sólo comportamiento
observable y su traducción a HLI.

---

## 1. Dos renderers

| | Claude Code classic | Claude Code fullscreen | HLI Agent Mode |
|---|---|---|---|
| Superficie | scrollback nativo | alternate screen | alternate screen |
| Composer | se desplaza con el output | fijo abajo | fijo abajo |
| Memoria | crece con la conversación | sólo lo visible | crece con la conversación |
| Ratón | lo gestiona el terminal | capturado por la app | capturado (desde este bloque) |
| Búsqueda | la del terminal | transcript viewer + `/` | transcript viewer + `/` |
| Volcado | implícito | `[` escribe al scrollback | no implementado |

HLI ya estaba en alternate screen sin captura de ratón: la conversación quedaba
encerrada y la rueda no hacía nada. Ese era el defecto real, no la falta de
PageUp.

## 2. Scroll y seguimiento

- La rueda mueve la conversación, no el cursor.
- Al subir se abandona el seguimiento; el contenido nuevo no mueve la vista.
- Un indicador cuenta lo que llega mientras se revisa historia y ofrece volver
  al final (`Ctrl+End` en HLI; en Claude Code el final se alcanza con la rueda o
  el atajo de salto).
- `PgUp`/`PgDn` mueven media pantalla; `Ctrl+Home`/`Ctrl+End` van a los extremos.
- La barra de desplazamiento representa posición y proporción, y sólo aparece
  cuando hay contenido fuera de la vista.

## 3. Transcript viewer

`Ctrl+O` abre una vista de revisión a pantalla completa, separada del panel de
administración. Permite buscar (`/`), recorrer con `j/k` y saltar entre bloques.
En HLI el equivalente es `TuiMode::Transcript`; el Inspector sigue siendo el
panel de runs, contexto, memoria y artefactos, y se abre con `Ctrl+I`.

## 4. Tool results

Colapsados por defecto; se expanden al seleccionarlos. Claude Code permite
click para expandir; HLI mantiene `Ctrl+E` y no implementa click en este bloque.

## 5. Selección y copia

Claude Code implementa selección interna con captura de ratón. HLI no la
implementa: ofrece `HLI_DISABLE_MOUSE=1` para devolver la selección nativa a
Windows Terminal y conservar el scroll por teclado.

## 6. Scrollback y export

Claude Code puede escribir la conversación al scrollback nativo (`[`). HLI no
lo implementa todavía: manipular el alternate screen a mitad de sesión es
arriesgado en Windows Terminal y no hay prueba concluyente. La alternativa
segura es exportar el transcript a un archivo (pendiente).

## 7. Restauración

Claude Code cuenta fallos de arranque del fullscreen y cae al classic. HLI usa
un guard RAII que desactiva la captura de ratón y restaura la pantalla al salir,
ante error y ante panic.

## 8. Diferencias que HLI conserva

- Un solo renderer (no hay classic todavía).
- Sin selección interna ni click.
- Sin volcado al scrollback.
- Localización en español de los controles.
- Transcript Mode propio, no una copia del Inspector.
