# HLI — Checklist de release Beta

## Automático

- [x] `cargo fmt --all -- --check`
- [x] `cargo clippy --workspace --all-targets --all-features -- -D warnings`
- [x] `cargo test --workspace` (debug)
- [x] `cargo test --workspace --release`
- [x] `cargo audit`
- [x] `cargo deny check`
- [x] Gates: MCP, módulos, supervisor MCH, firma, migraciones
- [x] Gate de empaquetado e instalación Windows
- [x] Quickstart automatizado sobre paquete instalado
- [x] Checksums del paquete verificados
- [x] Sin claves privadas en `dist/`
- [x] Sin procesos huérfanos del gate

## Manual (bloquea el tag)

- [ ] `docs/TUI-MANUAL-GATE.md` ejecutado y firmado por el usuario
- [ ] Resoluciones y terminal anotadas
- [ ] Resultado de los 19 pasos registrado

Estado: **beta package ready — tag pending manual TUI gate**.
