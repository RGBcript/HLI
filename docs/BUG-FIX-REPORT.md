# HLI — Informe de corrección de bugs

**Fecha:** 2026-11-07 · **Base:** `a740d34`

| # | Bug | Reproducción | Causa raíz | Corrección | Prueba | Commit |
|---|---|---|---|---|---|---|
| C1 | Path traversal en `hliar`/`.hlimod` | contenedor con `FILE n ../evil` | `read_container` no validaba rutas | `safe_rel_path` rechaza `..`, absolutas, unidad y UNC | `rejects_*` (5) | `fix(store)` |
| H1 | Checksums incompletos | `.hlimod` sin `manifest.json` en checksums | solo se verificaban las entradas listadas | exigir manifest y entry | — (código) | `fix(daemon)` |
| H2 | SDK laxo | `sdk: ">=9.0 <9.1"` instalaba | `contains("0.4")` | `sdk_allows` con rangos | `module_rejects_incompatible_sdk` | `fix(daemon)` |
| H3 | Error de proveedor deja run `queued` | `--base-url http://127.0.0.1:9` | `?` abortaba antes de `finish_run` | capturar error, `run.failed` + receipt | `provider_error_marks_run_failed` | `fix(daemon)` |
| H4 | Panic deja raw mode | `panic!` en TUI | sin hook de restauración | `install_panic_hook` | `terminal_is_restored_after_panic` | `fix(tui)` |
| M1 | `mch.invoke` sin ready | invocar antes de `serve` | no se comprobaba el estado | exigir `ready` | — (gate 0.4 hace serve) | `fix(daemon)` |
| M2 | Lockfile sin verificar | borrar una línea de `hli.lock` | no había comprobación | `module.verify` | — (comando) | `fix(daemon)` |
| M3 | Frame MCP sin límite | `Content-Length: 999999999` | reserva sin tope | límite 8 MiB | — (código) | `fix(mcp)` |
| M4 | TUI estados/idioma/header | abrir TUI vacía | lógica sin dependencias | estados dependientes, etiquetas, inglés, título, limpieza | `empty_state_respects_dependency_order`, `header_labels_all_none_values`, `language_is_consistent`, `inspector_title_matches_selection` | `fix(tui)` |
| L1 | Clippy | `cargo clippy -D warnings` | lints de estilo | correcciones puntuales | clippy limpio | `style` |
| L2 | Formato | `cargo fmt --check` | sin formatear | `cargo fmt --all` | fmt limpio | `style` |

## Pruebas de regresión añadidas

- `hli-store`: 5 (`rejects_dot_dot_path`, `rejects_absolute_path_outside_workspace`,
  `rejects_windows_drive_escape`, `rejects_unc_path_escape`, `rejects_hlimod_traversal`).
- `hli-daemon`: 2 (`accepts_compatible_sdk`, `module_rejects_incompatible_sdk`).
- `hli-cli`: 1 (`provider_error_marks_run_failed`).
- `hli-tui`: 5 (`empty_state_respects_dependency_order`, `header_labels_all_none_values`,
  `language_is_consistent`, `inspector_title_matches_selection`,
  `terminal_is_restored_after_panic`).

Total: **13 pruebas nuevas**.

## No corregido (declarado)

- `cargo audit`/`deny`: no instalados.
- `artifact.put` no confina la ruta de origen; es una operación local del usuario.
- Sin `proptest`.
- Sin prueba de TUI en terminal real.
