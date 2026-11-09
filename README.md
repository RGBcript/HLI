# HLI — Harness-Line Interface

> **Beta privada · 0.9.0-beta.1**
>
> HLI no está estable y este repositorio no lo pretende. Es una base de ingeniería
> que funciona, con pruebas y gates en verde, sobre la que se construirá el proyecto:
> una reconstrucción de un harness anterior desde cero, con un núcleo más pequeño y verificable.
>
> Lo que **sí** hace hoy: instalar, arrancar el daemon sola, ejecutar tareas con un
> modelo real (Ollama local o compatible con OpenAI), plan durable con efectos,
> permiso por invocación, receipts, MCP, MCH, TUI con pestañas y `/settings`.
>
> Lo que **no** hace todavía: leer `AGENTS.md` como instrucciones de proyecto,
> modos de permiso (`plan`, `accept-edits`, `dont-ask`), skills con carga diferida,
> compactación de contexto, checkpoints/rewind y streaming de la respuesta.
>
> El gate humano de la TUI sigue pendiente (`manual_pending=true`). No hay tag de
> versión estable ni paquete publicado. Si necesitas algo que puedas desplegar sin
> supervisión, esto no es.

HLI (**Harness-Line Interface**) es un agent harness local de peso medio, CLI-first,
MCP-native y autónomo. Convierte sesiones, modelos, herramientas y proveedores
heterogéneos en workspaces persistentes, ramificables y reutilizables. El mismo
núcleo se expone por CLI, MCP y, opcionalmente, ACP.

El nombre describe el punto medio: la interacción directa de una interfaz de
línea sobre la persistencia, modularidad y observabilidad de un harness completo.

- **MCH** (Modular Capability Harness): instancia especializada administrada por
  HLI, con proceso persistente, lifecycle, health y límite de reinicios.
- **MCP**: protocolo de interoperabilidad para clientes externos.
- **ACP**: adaptador opcional como agente para editores y hosts (experimental).
- **CLI**: interfaz textual (`hli`), cliente del daemon.
- **Module**: capacidad instalable (`.hlimod`, firma Ed25519 opcional).

**Versión:** 0.9.0-beta.1 · **Licencia:** Apache-2.0 · **Plataforma probada:**
Windows x86_64. Linux es experimental.

## Por qué beta y no 1.0

| Ya está | Falta para 1.0 |
|---|---|
| 405 pruebas en debug y release, 0 fallos | Gate humano de la TUI firmado |
| 11 gates en `ok=true` | `AGENTS.md` leído por el runtime |
| Event log, planes, permisos, receipts | Modos de permiso y reglas con glob |
| Installer, `doctor`, daemon automático | Compactación, checkpoints, streaming |
| TUI usable con modelo real | CI verde sostenido y paquete |

La lista de trabajo, con referencias de Command Code y otros CLIs, está en
[docs/AGENT-PROTOCOL-PROPOSAL.md](docs/AGENT-PROTOCOL-PROPOSAL.md).

## Para qué se construye

No para competir en funciones, sino para servir de base a **el proyecto**, la
reconstrucción de un harness anterior: un núcleo pequeño, con estado durable y permisos explícitos,
que sea fácil de auditar y de extender sin arrastrar la superficie de un producto
completo.

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

- [Propuesta de protocolo agéntico](docs/AGENT-PROTOCOL-PROPOSAL.md): AGENTS.md, modos de permiso,
  skills, checkpoints y subagentes, con las referencias de Command Code y otros CLIs.

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
