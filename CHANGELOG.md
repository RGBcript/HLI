# Changelog

## 0.9.0-beta.1 — 2026-11-07

Primera distribución pública instalable.

- Versión centralizada `0.9.0-beta.1`; `--version` en CLI, daemon y MCP.
- Licencia Apache-2.0, NOTICE e inventario de terceros.
- Paquete Windows x86_64 con instalador/desinstalador por usuario y checksums.
- Módulo `hli-memory` firmado con clave de release local (pública en el paquete).
- Gate de instalación limpia y gate Beta.
- CI Windows; Linux experimental.
- Corrección: el daemon ya no hereda los handles estándar del CLI al arrancar
  (evita bloqueos de automatización en PowerShell).
- Documentación pública: arquitectura, CLI, MCP, módulos, MCH, seguridad,
  empaquetado y migraciones.

## 0.9.0-beta.0 — interno

- Supervisor MCH cerrado (autostart y límite de reinicios).
- Firma Ed25519 de módulos.
- Migraciones versionadas.
