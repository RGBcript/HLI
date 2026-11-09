# HLI — Development Handbook

**Versión:** `0.9.0-beta.1` · **Repositorio:** `C:\Users\GUERRA\Documents\TEC\Doc\Software\HLI`

Cómo trabajar en el repositorio: crates, pruebas, gates, CI, convenciones,
versionado y proceso de release. Si vas a tocar código, esto es lo que hay que
leer antes.

---

## 1. Estructura del repositorio

```
HLI/
├── Cargo.toml            workspace (11 crates, edition 2024, Apache-2.0)
├── crates/               código
├── docs/                 57 documentos + docs/adr/
├── scripts/              gates, empaquetado, instalación (17 archivos)
├── modules/              fuentes de módulos empaquetables
├── release-notes/        notas por versión
├── dist/                 paquete generado (ignorado por git)
├── target/               build (ignorado por git)
├── .github/workflows/    ci.yml, release.yml
├── README.md · QUICKSTART.md · CHANGELOG.md
├── SECURITY.md · THREAT-MODEL.md · CONTRIBUTING.md
└── LICENSE · NOTICE · THIRD-PARTY-NOTICES.md
```

Ignorados: `/target`, `dist/`, `*.db*`, `*.hliar`, `daemon.json`, `daemon.log`.
**No** se versionan bases ni artefactos de build.

---

## 2. Crates

| Crate | Rol | Depende de | Líneas en `src/` |
|---|---|---|---:|
| `hli-core` | Canon JCS, IDs, errores, tiempo, versiones | — | 685 |
| `hli-store` | SQLite, migraciones, writer/reader, export | core | 4.273 |
| `hli-agent` | Agent loop, manifiesto, receipt | core, store, provider | 483 |
| `hli-provider` | sim, Ollama, OpenAI-compatible, precios | core | 365 |
| `hli-signing` | Ed25519, trust, políticas, paquetes | core | 896 |
| `hli-daemon` | Despacho, supervisor MCH, providers | todas las anteriores | 2.532 |
| `hli-cli` | Cliente CLI | core | 909 |
| `hli-tui` | Cliente TUI (ratatui/crossterm) | core | 1.291 |
| `hli-mcp` | Servidor MCP stdio | core | 322 |
| `hli-module-memory` | Módulo de memoria (proceso hijo) | core | 186 |
| `hli-module-fixture` | Módulo de prueba para gates | core | 225 |

Regla de dependencias: **nada importa `hli-daemon`**. La CLI, la TUI y MCP hablan
por el protocolo local, no por la API interna del daemon. `hli-tui` tiene una
prueba que falla si aparece `rusqlite` o `hli-store` en su `Cargo.toml`.

`hli-daemon/src/main.rs` es el mayor monolito (2.532 líneas). Si añades comandos,
considera extraer módulos antes de que crezca más.

---

## 3. Pruebas

```powershell
cargo test --workspace                     # 134 pruebas, debug
cargo build --release --workspace          # los e2e lanzan el binario release
cargo test --workspace --release           # 134 pruebas, release
```

Distribución actual: `hli-cli` 29, `hli-tui` 28, `hli-store` 23, `hli-daemon` 22,
`hli-signing` 12, `hli-core` 10, `hli-agent` 7, `hli-provider` 3. `hli-mcp`,
`hli-module-memory` y `hli-module-fixture` no tienen pruebas unitarias: su
cobertura vive en los gates.

Convenciones:

- Los e2e se aíslan con `HLI_HOME` propio por prueba; nunca usar el home real.
- Los tests de tiempo usan políticas en milisegundos inyectadas, no esperas de
  producción.
- Un test nuevo debe nombrar el comportamiento, no el método:
  `mch_fourth_restart_marks_failed`, no `test_restart_4`.
- Si un test necesita el daemon, lo arranca y lo para en el propio test.

---

## 4. Gates

Un gate es un script externo que demuestra una propiedad **con binarios reales**.
No importa los crates: habla CLI, daemon, MCP o el paquete instalado. Todos
devuelven JSON y código de salida distinto de cero si fallan.

| Script | Lenguaje | Cubre |
|---|---|---|
| `scripts/mcp_gate.py` | Python | Cliente MCP externo, trayectoria, prompt, memoria, receipt, reconexión, export/import |
| `scripts/module_gate.py` | Python | Empaquetar, instalar, probar, MCH, CLI/MCP, reinicio, checksums, SDK |
| `scripts/mch_supervisor_gate.py` | Python | Lifecycle, persistencia, PID, degradación, reinicio, autostart, límite |
| `scripts/module_signing_gate.py` | Python | Clave, trust, firma, tamper, revocación |
| `scripts/migration_gate.py` | Python | Fixture previa, backup, apply, verify, rollback |
| `scripts/install_gate.ps1` | PowerShell | Instalación limpia del paquete |
| `scripts/beta_gate.ps1` | PowerShell | Agregado + fmt/clippy/tests/audit/deny/empaquetado |

Ejecución individual:

```powershell
python scripts/mch_supervisor_gate.py
python scripts/migration_gate.py
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/install_gate.ps1 -Version 0.9.0-beta.1
```

Ejecución completa (tarda; deja logs con `-KeepLogs`):

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/beta_gate.ps1 -KeepLogs
```

### 4.1 El gate manual de la TUI

`beta_gate.ps1` fija `tui_manual = $false` y por eso `ok` nunca es `true`. El
valor solo debe cambiar cuando exista una firma real en
`docs/TUI-MANUAL-GATE.md`.

Estado verificado el 2026-11-07 con un arnés PTY (ConPTY + pyte):

- Los pasos 1–3, 6–12, 17–19 son ejecutables tal como están escritos.
- **Paso 4**: el ciclo de foco es Navigation → Collection → Inspector → Tabs →
  Command, pero en pantalla solo se ven los títulos de panel (`Runs`,
  `Run <id>`, pestaña activa). Las etiquetas `Collection`/`Inspector`/`Tabs` solo
  aparecen en el layout Compact.
- **Paso 5**: tras 4 `Tab` el foco está en `CommandBar`, donde `1..6` se escriben
  como texto. Hay que pulsar `Esc` antes de usar los dígitos.
- **Paso 13**: `<60` columnas entra en layout **Reduced** (solo inspector); el
  indicador `n/5` aparece entre 60 y 89 columnas (Compact).

Corregir el guion (o el código) es requisito para poder firmar el gate.

---

## 5. CI

`.github/workflows/ci.yml`

- **Windows (soporte Beta):** version, `fmt --check`, `clippy -D warnings`,
  `cargo test --workspace`, `cargo audit`, `cargo deny check`.
- **Linux (experimental):** solo núcleo (fmt, clippy, tests, audit, deny). Los
  gates de integración no están portados.

`.github/workflows/release.yml`

- Se dispara con tags `v*`. Construye el paquete y publica.

Antes de abrir un PR, reproducir localmente lo que corre CI:

```powershell
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace
cargo audit
cargo deny check
```

---

## 6. Convenciones

**Código**

- Rust edition 2024; `rustfmt` sin configuración propia.
- `clippy` con `-D warnings`: no se aceptan warnings.
- Errores con `HliError` tipado; los mensajes de usuario en español, con código
  cuando aplique (`invalid`, `not_found`, `conflict`).
- Sin `unwrap()` en rutas de producción; en tests se permite.
- Serialización JSON canónica a través de `hli-core::canon`; no improvisar
  normalización.

**Commits**

Prefijo convencional y mensaje imperativo en inglés, como el historial actual:

```
feat(store): add versioned transactional migrations
fix(tui): restore terminal after panic
docs: add state, architecture, operations and beta review
```

**Documentación**

- Toda decisión que cambie contrato va a `docs/` y a `CHANGELOG.md`.
- Las decisiones estructurales van a `docs/adr/NNNN-titulo.md` con contexto,
  decisión y consecuencias.
- Los límites se documentan; no se ocultan.

**Prohibido**

- Falsificar gates, saltar tests o silenciar warnings sin justificar.
- Incluir claves privadas, secretos, bases de datos o `dist/` en el repo.
- Añadir dependencias pesadas sin necesidad demostrable.

---

## 7. ADRs

Ubicación: `docs/adr/`. Estados posibles: propuesta, aceptada, sustituida.

| ADR | Tema |
|---|---|
| 0001 | Transporte local TCP + NDJSON |
| 0002 | `previous_hash` en el sobre canónico |
| 0003 | JCS como subconjunto |
| 0004 | Contenedor `hliar` provisional |
| 0005 | Alcance de 0.1 |
| 0006 | Run, ContextManifest y agent loop |
| 0007 | Trayectoria de rama, prompt cubierto, memoria y MCP |
| 0008 | Module Protocol, `.hlimod` y MCH |

Faltan ADRs para firma Ed25519, migraciones versionadas, empaquetado Windows y
autostart del supervisor. Escribirlos es deuda registrada.

Plantilla mínima:

```markdown
# ADR-NNNN — Título

**Estado:** aceptada · **Fecha:** YYYY-MM-DD

## Contexto
## Decisión
## Consecuencias
## Alternativas descartadas
```

---

## 8. Versionado

- **Fuente única:** `[workspace.package] version` en `Cargo.toml`. Los crates
  heredan con `version.workspace = true`.
- `--version` debe coincidir en `hli`, `hli-daemon` y `hli-mcp`.
- El `.hlimod` del módulo lleva su propia versión (`0.9.0-beta.1` en el paquete).
- `CHANGELOG.md` por bloque funcional; `release-notes/vX.Y.Z.md` para lo publicado.
- Esquema de base: `DATABASE_SCHEMA_VERSION` en `hli-core/src/versions.rs` (hoy 7).
  Subir el esquema **exige** una migración nueva y su gate.

Semver durante la Beta: `0.9.0-beta.N`. Un cambio incompatible sube `0.10.0`.

---

## 9. Proceso de release

1. **Código verde**
   ```powershell
   cargo fmt --all -- --check
   cargo clippy --workspace --all-targets --all-features -- -D warnings
   cargo build --release --workspace
   cargo test --workspace
   cargo test --workspace --release
   ```
2. **Gates**
   ```powershell
   python scripts/mcp_gate.py
   python scripts/module_gate.py
   python scripts/mch_supervisor_gate.py
   python scripts/module_signing_gate.py
   python scripts/migration_gate.py
   powershell -NoProfile -ExecutionPolicy Bypass -File scripts/install_gate.ps1 -Version 0.9.0-beta.1
   ```
3. **Paquete**
   ```powershell
   powershell -NoProfile -ExecutionPolicy Bypass -File scripts/package_windows.ps1 -Version 0.9.0-beta.1
   ```
   Genera `dist/hli-<version>-windows-x86_64/` con binarios, `.hlimod` firmado,
   clave pública, instalador/desinstalador, docs y `checksums.sha256`.
4. **Gate Beta completo** con `-KeepLogs` y revisar el JSON: `ok` debe ser `true`.
5. **Gate visual manual**: 19 pasos en Windows Terminal, firmar
   `docs/TUI-MANUAL-GATE.md` §6.
6. **Commit y tag**
   ```powershell
   git tag -a v0.9.0-beta.1 -m "HLI 0.9.0-beta.1"
   git push origin v0.9.0-beta.1
   ```
7. **Publicación**: `release.yml` se dispara con el tag; adjuntar el paquete y
   `checksums.sha256`.

Regla dura: **no se crea tag si `beta_gate.ps1` devuelve `ok=false`**. El tag
declara una beta que existe, no una que se espera.

---

## 10. Añadir una capacidad (receta)

1. **Modelo**: ¿cambia el esquema? → migración nueva + fixture + gate.
2. **Core/store**: tipos, validaciones, escritura transaccional, prueba unitaria.
3. **Daemon**: comando nuevo en el despacho, con validación de argumentos.
4. **CLI**: subcomando + `--output json` + entrada en la ayuda.
5. **TUI** (si aplica): campo en `App`, refresco, render, prueba de layout.
6. **MCP** (si aplica): tool nueva con `inputSchema` y `required`.
7. **Pruebas**: unitaria + e2e con `HLI_HOME` aislado.
8. **Gates**: ampliar el gate correspondiente si la capacidad es verificable.
9. **Docs**: guía de uso, `CHANGELOG.md`, ADR si cambia contrato.

Si el paso 1 es "no", probablemente no hace falta tocar el esquema; si el paso 3
mete lógica de negocio en el despacho, se está engordando el monolito.

---

## Planificación durable

- Reglas del dominio en `hli-core` (hashes y transiciones) y `hli-agent`
  (planner). El daemon no decide la política: la aplica.
- Todo cambio en el plan debe pasar por `RunPlan::seal()`; el hash cubre sólo
  contenido revisado (no estado ni aprobación).
- Pruebas: `crates/hli-agent/src/plan.rs`, `crates/hli-daemon/tests/planning_approval.rs`,
  `crates/hli-tui/tests/render.rs` y `scripts/planning_approval_gate.py`.
- No añadir campos privados al plan: la instantánea de ejecución es el único
  lugar para configuración, y nunca guarda valores de secreto.

---

## Permisos por invocación

- La política vive en `hli-agent`; el daemon sólo la aplica.
- Toda decisión debe pasar por `decide_tool_request`: validar estado, sellar el
  request, crear grant (run/workspace) y emitir evento en la misma transacción.
- Al reanudar, comparar `invocation_hash` con `arguments_hash`; nunca confiar en
  los argumentos sin verificar.
- Pruebas: `hli-agent/src/permissions.rs`, `planning_approval` del daemon,
  `render` de la TUI y `scripts/tool_permission_gate.py`.
