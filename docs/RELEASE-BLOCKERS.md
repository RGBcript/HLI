# HLI — Release blockers

**Código auditado:** `6a869d4` · **Fecha:** 2026-11-07 · **Veredicto:** RC READY WITH ACCEPTED LIMITATIONS

## Bloqueantes corregidos

| Severidad | Bug | Estado |
|---|---|---|
| Crítico | Path traversal en `hliar`/`.hlimod` | corregido, 5 pruebas |
| Alto | Checksums incompletos en `module.install` | corregido |
| Alto | Rango de SDK laxo | corregido, 2 pruebas |
| Alto | Error de proveedor dejaba run `queued` | corregido, 1 prueba |
| Alto | Panic dejaba el terminal en raw mode | corregido, 1 prueba |
| Medio | `mch.invoke` sin `ready` | corregido |
| Medio | Lockfile sin verificación | corregido (`module.verify`) |
| Medio | Frame MCP sin límite | corregido (8 MiB) |
| Medio | TUI: estados, idioma, cabecera, inspector, selección | corregido, 4 pruebas |
| Bajo | Clippy y formato | corregido |

## Verificación final

- `cargo fmt --check`: sin diff.
- `cargo clippy -D warnings`: 0 errores.
- `cargo test --workspace`: 73 pruebas en verde.
- `cargo test --workspace --release`: 73 en verde (requiere `cargo build --release --workspace` previo, porque los e2e lanzan el binario del daemon).
- Gates 0.3 y 0.4: ok.

## Auditoría de dependencias

`cargo audit` (cargo-audit 0.22.2): **0 vulnerabilidades**, 3 avisos.
`cargo deny check` (cargo-deny 0.20.2): **advisories ok, bans ok, licenses ok, sources ok**, con los tres avisos ignorados de forma justificada en `deny.toml`.

| ID | Crate | Tipo | Dependencia | Alcanzabilidad | Parche | Decisión |
|---|---|---|---|---|---|---|
| RUSTSEC-2024-0436 | paste 1.0.15 | unmaintained | ratatui 0.29 → hli-tui | proc-macro de compilación; HLI no lo invoca | no; crate archivado, alternativa `pastey` vía ratatui nuevo | aceptar |
| RUSTSEC-2026-0002 | lru 0.12.5 | unsound (`IterMut`) | ratatui 0.29 → hli-tui | HLI no usa `LruCache` ni `IterMut`; uso interno de ratatui no reproducido | no en 0.12; requiere ratatui con lru 0.19 | aceptar con revisión |
| RUSTSEC-2026-0253 | lru 0.12.5 | unsound (`pop()`) | ratatui 0.29 → hli-tui | HLI no llama `LruCache::pop`; uso interno no reproducido | no en 0.12 | aceptar con revisión |

No se actualizaron dependencias de forma masiva. Los tres avisos son transitivos de `ratatui 0.29` y solo afectan a `hli-tui`; se revisan al actualizar ratatui, no antes.

## Limitaciones aceptadas

- **Gate visual manual de la TUI:** guía y entorno preparados (`docs/TUI-MANUAL-GATE.md`), pero la ejecución en Windows Terminal depende del usuario. No se declara pasado.
- **Avisos transitivos de ratatui:** dos `unsound` en `lru 0.12.5` y uno de mantenimiento en `paste`, sin parche dentro de la rama 0.12; no alcanzables directamente.
- **`artifact.put`:** no confina la ruta de origen (operación local del usuario).
- **`read_container`/`import`:** sin límite de tamaño; un paquete enorme consume memoria.
- **`shell.execute`:** denegado; no hay sandbox de SO.
- **TUI:** sin streaming; polling 500 ms / 1 s / 4 s.
- **Preferencias TOML:** no implementadas.
- **`Alt+1..6`:** documentado, no implementado.

## No evaluable

- Auditoría de dependencias de terceros más allá de RustSec (sin SBOM ni análisis de código de crates).
- Integración con un harness anterior: su configuración 0.2.1-rc.5 no expone MCP.
- Gate visual en terminal real: pendiente de ejecución manual.

## Recomendación

**RC READY WITH ACCEPTED LIMITATIONS.** No queda ningún bug crítico o alto reproducible. Las limitaciones son de evaluación (gate visual manual) y de dependencias transitivas sin parche en la rama actual. Esta comprobación autoriza una Release Candidate, no una publicación estable.
