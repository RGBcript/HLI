# HLI — Hierarchical Layered Intelligence

HLI es un **runtime agéntico completo**: ejecuta turnos con contexto, memoria,
eventos verificables, módulos y procesos especializados, todo sobre un almacén
local.

- **MCH**: instancia especializada administrada por HLI (proceso persistente con
  lifecycle, health y límite de reinicios).
- **MCP**: protocolo de interoperabilidad para clientes externos.
- **CLI**: interfaz textual (`hli`), cliente del daemon.
- **Module**: capacidad instalable (`.hlimod`, firma Ed25519 opcional).

**Versión:** 0.9.0-beta.1 · **Licencia:** Apache-2.0 · **Plataforma probada:**
Windows x86_64. Linux es experimental.

## Instalación rápida

```powershell
powershell -ExecutionPolicy Bypass -File .\install.ps1
# abre una terminal nueva
hli doctor
hli
```

El instalador compila en release y copia `hli`, `hli-tui`, `hli-daemon` y
`hli-mcp` a `%USERPROFILE%\.local\bin`, añade esa carpeta al PATH del usuario y
verifica con `hli --version`. Sin administrador. En Linux/macOS: `./install.sh`.

El daemon arranca solo en el primer comando; el workspace del directorio se crea
también sin pasos previos. Una pasada sin TUI:

```bash
hli "resume este repo"          # o hli -p "resume este repo"
cat log.txt | hli -p "¿qué falla?"
```

Guía completa: [docs/INSTALL.md](docs/INSTALL.md) ·
[QUICKSTART.md](QUICKSTART.md).

## Estado honesto

| Área | Estado |
|---|---|
| Runtime, eventos, snapshots, export/import | Implementado |
| Runs síncronos y asíncronos con cancelación/timeout | Implementado |
| Proveedores `sim`, Ollama y OpenAI-compatible | Implementado; requieren entorno |
| MCH persistente, health, autostart, límite de reinicios | Implementado |
| Firma Ed25519 de módulos y trust store | Implementado |
| Migraciones versionadas y backup | Implementado |
| MCP stdio con tools básicas | Implementado; no universal |
| TUI | Implementado; gate visual manual pendiente |
| Paquete Windows e instalador | Implementado y probado |
| Linux | Experimental, sin gates de integración portados |
| Sandbox / shell seguro | **No soportado** |
| Routing económico general | **No soportado** |
| Compatibilidad MCP universal | **No soportado** |

`shell.execute` está denegado por diseño. No se anuncia aislamiento completo del
sistema operativo.

## Agent Mode

`hli` sin argumentos abre **Agent Mode**: vincula el directorio actual a un
workspace, toma una tarea en lenguaje natural y crea el thread, la rama y el run
sin pedir nada más. `hli inspect` (o `Ctrl+O`) abre el dashboard de observabilidad
anterior, que se conserva como **Inspector**.

- Guía: [docs/AGENT-MODE.md](docs/AGENT-MODE.md) — incluye `hli resume`, `Ctrl+R`
  y el transcript durable desde el Event Store.
- Flujo: [docs/DELEGATION-WORKFLOW.md](docs/DELEGATION-WORKFLOW.md)
- Gate manual: [docs/AGENT-TUI-MANUAL-GATE.md](docs/AGENT-TUI-MANUAL-GATE.md)

Automatización y control directo siguen disponibles por CLI (`hli run "<tarea>"`,
`hli <recurso> <acción>`); MCP y MCH no cambian.

## Documentación de estado

Los cinco documentos oficiales del proyecto. Si solo vas a leer algo, lee el
primero; si vas a operar o desarrollar, lee el que corresponda.

- [HLI-STATE-OF-THE-PROJECT.md](HLI-STATE-OF-THE-PROJECT.md) — fotografía oficial:
  componentes, pruebas, gates, límites, roadmap y mapa del sistema.
- [HLI-ARCHITECTURE-HANDBOOK.md](HLI-ARCHITECTURE-HANDBOOK.md) — entidades,
  responsabilidades, invariantes y relaciones.
- [HLI-OPERATIONS.md](HLI-OPERATIONS.md) — instalación, diagnóstico, backups,
  migraciones, recovery, export/import, firmas, providers y MCH.
- [HLI-DEVELOPMENT.md](HLI-DEVELOPMENT.md) — crates, pruebas, gates, CI,
  convenciones, ADRs, versionado y release.
- [HLI-BETA-REVIEW.md](HLI-BETA-REVIEW.md) — revisión honesta: implementado,
  parcial, pendiente, no probado, riesgos y deuda técnica.

## Documentación

- [QUICKSTART.md](QUICKSTART.md)
- [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md)
- [docs/CLI-REFERENCE.md](docs/CLI-REFERENCE.md)
- [docs/MCP-GUIDE.md](docs/MCP-GUIDE.md)
- [docs/MODULE-DEVELOPMENT.md](docs/MODULE-DEVELOPMENT.md)
- [docs/MCH-GUIDE.md](docs/MCH-GUIDE.md)
- [docs/PROVIDER-GUIDE.md](docs/PROVIDER-GUIDE.md)
- [docs/MIGRATION-GUIDE.md](docs/MIGRATION-GUIDE.md)
- [docs/MODULE-SIGNING.md](docs/MODULE-SIGNING.md)
- [docs/PACKAGING.md](docs/PACKAGING.md)
- [SECURITY.md](SECURITY.md) · [THREAT-MODEL.md](THREAT-MODEL.md)
- [CONTRIBUTING.md](CONTRIBUTING.md) · [SUPPORT.md](SUPPORT.md)
- [CHANGELOG.md](CHANGELOG.md)

## Desarrollo

```powershell
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace
cargo build --release --workspace
```

Gates externos: `scripts/mcp_gate.py`, `module_gate.py`,
`mch_supervisor_gate.py`, `module_signing_gate.py`, `migration_gate.py`,
`package_windows.ps1`, `install_gate.ps1`, `beta_gate.ps1`.

## Licencia

Apache License 2.0. Ver [LICENSE](LICENSE), [NOTICE](NOTICE) y
[THIRD-PARTY-NOTICES.md](THIRD-PARTY-NOTICES.md).
