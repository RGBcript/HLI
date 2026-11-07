# HLI — Desarrollo de módulos

Un `.hlimod` es un contenedor `HLIAR1` con:

- `manifest.json` (`metadata.name/version`, `spec.sdk`, `runtime.entry`,
  `operations`, `lifecycle`).
- `schemas/` con los JSON Schema de las operaciones.
- `bin/<ejecutable>`.
- `checksums.json` con `sha256:` por archivo.
- `signature.json` opcional (Ed25519).

El módulo habla JSON-RPC 2.0 por stdio: `initialize`, `capabilities`,
`configure`, `invoke`, `health`, `shutdown` (y opcionalmente `cancel`,
`checkpoint`). HLI valida que las operaciones declaradas en el manifest existan
en `capabilities`.

Empaquetado de referencia: `crates/hli-module-memory` y `modules/hli-memory`.
Firma y verificación: `docs/MODULE-SIGNING.md`.

No incluyas secretos, claves privadas ni rutas absolutas del desarrollador en el
paquete.
