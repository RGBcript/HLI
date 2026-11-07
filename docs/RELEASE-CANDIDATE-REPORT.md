# HLI — Informe de Release Candidate

**Código:** `6a869d4` · **Fecha:** 2026-11-07 · **Alcance:** verificación de RC, sin cambios de arquitectura.

## 1. Identidad

| Ítem | Valor |
|---|---|
| Repositorio | `C:\Users\GUERRA\Documents\TEC\HLI` |
| HEAD de código | `6a869d4` |
| Versión | HLI 0.5 |
| Rust | estable del entorno |
| SO | Windows 11 |

## 2. Qué se verificó

| Comprobación | Comando | Resultado |
|---|---|---|
| Formato | `cargo fmt --all -- --check` | sin diff |
| Clippy | `cargo clippy --workspace --all-targets --all-features -- -D warnings` | 0 errores |
| Tests debug | `cargo test --workspace` | 73 passed, 0 failed |
| Tests release | `cargo build --release --workspace && cargo test --workspace --release` | 73 passed, 0 failed |
| Gate MCP | `python scripts/mcp_gate.py` | `{"gate":"0.3","ok":true}` |
| Gate módulo | `python scripts/module_gate.py` | `{"gate":"0.4","ok":true}` |
| Auditoría | `cargo audit` | 0 vulnerabilidades, 3 avisos |
| Política | `cargo deny check` | advisories ok, bans ok, licenses ok, sources ok |

`cargo test --release` requiere `cargo build --release --workspace` previo porque los e2e lanzan `hli-daemon.exe`; no es un defecto de código.

## 3. Dependencias

Las tres avisos son transitivas de `ratatui 0.29` y afectan solo a `hli-tui`:

- `paste 1.0.15` (unmaintained, sin vulnerabilidad).
- `lru 0.12.5` (unsound en `IterMut` y `pop()`), sin parche dentro de 0.12.

No hay actualización puntual disponible sin subir ratatui; no se hizo actualización masiva. `deny.toml` documenta la excepción con razón. Detalle en `RELEASE-BLOCKERS.md`.

## 4. Gate visual manual de la TUI

**Estado: pendiente de ejecución por el usuario.** La guía completa está en `docs/TUI-MANUAL-GATE.md`.

Entorno ya preparado en `%TEMP%\hli-rc-visual`:

- workspace `ws_aadea77f2cbc73cc`, thread `thr_c9c441519621242c`, branch `br_e4a4ec9b67f93adc`.
- run `run_6d30f034f6f0361d` (`sim`).
- módulo `hli-memory` 0.4.0 instalado y MCH `memory-main` en `ready`.
- daemon activo.

Abrir:

```powershell
$HLI = "C:\Users\GUERRA\Documents\TEC\HLI\target\debug\hli.exe"
$env:HLI_HOME = "$env:TEMP\hli-rc-visual"
& $HLI
```

Registrar los 19 pasos de la tabla: terminal y versión, dimensiones, foco inicial, creación de workspace/thread/branch, ejecución del run, navegación de pestañas, `NO_COLOR`, layout compacto, cierre normal, restauración del terminal y daemon activo.

## 5. Riesgos residuales

| Riesgo | Impacto | Mitigación |
|---|---|---|
| `lru` unsound transitivo | bajo; no reproducible desde HLI | revisar al subir ratatui |
| `paste` sin mantenimiento | nulo funcional | reemplazo vía ratatui |
| Gate visual no ejecutado | medio para UX | guía y entorno listos |
| Paquetes sin límite de tamaño | bajo; local | límite futuro |
| `artifact.put` sin confinar | bajo; local | operación del usuario |

## 6. Veredicto

**RC READY WITH ACCEPTED LIMITATIONS.**

Motivos:

1. No queda ningún bug crítico o alto reproducible; los 11 hallazgos de la auditoría están corregidos y con pruebas.
2. `cargo audit` no reporta vulnerabilidades; las tres avisos son transitivas, no alcanzables y sin parche en la rama actual.
3. `cargo deny check` pasa con excepciones documentadas.
4. El gate visual de la TUI está preparado pero no ejecutado; se acepta como limitación de esta comprobación.

Esta comprobación **no autoriza una publicación estable**. Autoriza una Release Candidate.
