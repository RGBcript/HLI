# HLI — Auditoría integral de código

**Commit inicial:** `a740d34` · **Árbol inicial:** limpio · **Fecha:** 2026-11-07

## 1. Alcance

Todo el workspace: `hli-core`, `hli-store`, `hli-agent`, `hli-provider`, `hli-daemon`,
`hli-cli`, `hli-mcp`, `hli-module-memory`, `hli-tui`, `scripts/`.

No se añadieron capacidades. Se corrigieron defectos confirmados por código o reproducción.

## 2. Metodología

1. Línea base: `git status`, `HEAD`, `cargo build`, `cargo test`, `mcp_gate`, `module_gate`.
2. Estática: `cargo fmt --check`, `cargo clippy -D warnings`, búsqueda de `unwrap`, `expect`,
   `panic!`, `unsafe`, `process::exit`, SQL dinámico, rutas desde input.
3. Revisión manual de persistencia, runs/contexto, MCP, módulo, seguridad y TUI.
4. Reproducción y prueba de regresión por bug.
5. Verificación final.

## 3. Línea base

| Ítem | Valor |
|---|---|
| Commit | `a740d34` |
| Árbol | limpio |
| Build | ok |
| Tests | 20 bloques en verde |
| Gates 0.3/0.4 | ok |
| `cargo fmt --check` | **diff** (no formateado) |
| `cargo clippy -D warnings` | **5 errores** (estilo) |

## 4. Hallazgos por severidad

### Crítico

**C1. Path traversal en contenedores `hliar`/`.hlimod`.**
`read_container` aceptaba rutas como `../../evil`; `module.install` escribía `dir.join(path)` y
`import` escribía `artifacts_dir.join(path)`. Un paquete manipulado podía escribir fuera de
`HLI_HOME`. Corregido con `safe_rel_path` en el parser, que rechaza `..`, rutas absolutas,
unidades Windows y UNC. Pruebas: `rejects_dot_dot_path`, `rejects_absolute_path_outside_workspace`,
`rejects_windows_drive_escape`, `rejects_unc_path_escape`, `rejects_hlimod_traversal`.

### Alto

**H1. Checksums incompletos.** `module.install` verificaba solo los archivos listados en
`checksums.json`; un manifiesto o binario no listado se instalaba sin verificar. Ahora exige
`manifest.json` y el `runtime.entry` en los checksums.

**H2. Rango de SDK laxo.** `sdk.contains("0.4")` aceptaba `>=9.0 <0.4`. Nueva `sdk_allows` que
interpreta `>=`, `<` y `^`. Pruebas: `accepts_compatible_sdk`, `module_rejects_incompatible_sdk`.

**H3. Error de proveedor dejaba el run en `queued`.** El `?` en `p.chat()` abortaba antes de
`finish_run`; no había `run.failed` ni receipt. Ahora el error se captura, se emite
`run.failed` y el receipt se genera con `usage` vacío y `result.error`. Prueba:
`provider_error_marks_run_failed`.

**H4. Panic dejaba el terminal en raw mode.** `ratatui::init/restore` no cubre panics. Se añadió
`install_panic_hook` que restaura antes del hook por defecto. Prueba:
`terminal_is_restored_after_panic`.

### Medio

**M1. `mch.invoke` sin comprobar `ready`.** Invocaba una instancia `configured` o `stopped`.
Ahora exige `state == ready`.

**M2. Lockfile sin verificación.** `hli.lock` se escribía pero no se comprobaba. Nuevo
`module.verify` que compara los checksums de la base con el lockfile.

**M3. Frame MCP sin límite.** `Content-Length` arbitrario podía reservar memoria sin límite.
Ahora se rechaza > 8 MiB.

**M4. TUI: estados vacíos y idioma.**
- El panel Runs sugería `:run start` sin workspace/thread/branch. Ahora respeta la dependencia:
  workspace → thread → branch → run.
- Cabecera `workspace: none | none/none` sustituida por etiquetas explícitas.
- Mezcla de español e inglés; unificada a inglés técnico.
- El inspector mostraba `Memory ...` con un run seleccionado; ahora el título corresponde al
  run o `No selection`.
- `refresh_runs` dejaba un run fantasma si la lista quedaba vacía; ahora limpia la selección.

### Bajo

**L1. Clippy:** import sin uso, `as_bytes().len()`, `&PathBuf`, comparación de longitud, `format!`
sin argumentos, `Default` ausente, `let _ = ()`. Corregidos.

**L2. `cargo fmt`:** todo el workspace reformateado.

## 5. Falsos positivos

- `unwrap()` en tests: aceptable; el fallo debe ser ruidoso.
- `process::exit(0)` en `daemon.stop`: intencional y documentado (ADR/0001); el CLI espera a que
  el ping falle.
- `expect` en arranque de hilos: si falla el spawn, el proceso no puede continuar.
- `unsafe`: no hay.
- SQL dinámico: solo `format!` con nombres de columna constantes y `EVENT_COLS`/`RUN_COLS`;
  los valores van por parámetros.

## 6. Limitaciones

- `cargo audit` y `cargo deny` no están instalados; no se ejecutaron.
- No se usó `proptest`; las pruebas son de regresión dirigidas.
- La TUI no se probó en terminal real; solo `TestBackend`.
- No se auditó el código de dependencias (ratatui, crossterm, ureq, rusqlite).
- `artifact.put` acepta rutas locales del usuario; no es una frontera remota, pero no se confina.

## 7. Evidencia

- `cargo clippy --workspace --all-targets --all-features -- -D warnings`: 0 errores.
- `cargo fmt --all -- --check`: sin diff.
- `cargo test --workspace`: 20 bloques, 73 pruebas en verde.
- `cargo test --workspace --release`: 73 pruebas en verde, tras `cargo build --release --workspace` (los e2e lanzan el binario del daemon).
- `python scripts/mcp_gate.py`: `{"gate":"0.3","ok":true}`.
- `python scripts/module_gate.py`: `{"gate":"0.4","ok":true}`.
