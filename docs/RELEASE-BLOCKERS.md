# HLI — Release blockers

**Commit auditado:** `a740d34` · **Fecha:** 2026-11-07

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

## Bloqueantes abiertos

Ninguno **critical** o **high** reproducible en el código actual.

## Deuda no bloqueante

- `cargo audit` y `cargo deny` no instalados; no se ejecutaron.
- Sin `proptest`; propiedades cubiertas por pruebas dirigidas.
- `artifact.put` no confina la ruta de origen (operación local del usuario).
- Sin límite de tamaño en `read_container`/`import` (un paquete enorme consume memoria).
- `shell.execute` sigue denegado; no hay sandbox de SO.
- TUI sin streaming; polling 500 ms/1 s/4 s.
- Preferencias TOML no implementadas.
- `Alt+1..6` documentado, no implementado.

## No evaluable

- Gate visual de la TUI en terminal real: el entorno no ofrece TTY estable.
- Integración con un harness anterior: su configuración no expone MCP en 0.2.1-rc.5.
- Auditoría de dependencias de terceros.
- `cargo test --release`: no ejecutado en esta sesión por tiempo; el build release no se probó.

## Recomendación

**Release candidate: sí, con salvedades.**

No queda ningún bug crítico o alto reproducible. Las salvedades son de evaluación, no de código:
no se ejecutaron `cargo audit`/`deny`, no se probó `--release` y el gate visual de la TUI sigue
pendiente de una terminal real. Para publicar, ejecutar esos tres puntos y repetir los gates.
