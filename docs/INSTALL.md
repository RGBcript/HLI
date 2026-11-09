# Instalar HLI

HLI se instala en el perfil del usuario, como Claude Code: sin administrador,
sin `sudo`, en una sola carpeta, y deja el comando `hli` disponible en cualquier
terminal.

## Requisitos

- Windows 10/11 (x64), Linux o macOS.
- Rust estable (`cargo`) para compilar desde el repositorio.
- Un proveedor: `sim` viene incluido; `ollama` u `openai` son opcionales.

## Instalación

Desde la raíz del repositorio:

**Windows (PowerShell):**

```powershell
powershell -ExecutionPolicy Bypass -File .\install.ps1
```

**Linux / macOS / Git Bash:**

```bash
./install.sh
```

El instalador:

1. compila `hli`, `hli-tui`, `hli-daemon` y `hli-mcp` en release;
2. los copia juntos a `~/.local/bin` (`%USERPROFILE%\.local\bin` en Windows);
3. añade esa carpeta al PATH del usuario si falta;
4. ejecuta `hli --version` para verificar.

Abre una terminal **nueva** y comprueba:

```bash
hli doctor
```

`hli doctor` es de solo lectura: muestra versión, ejecutable, `HLI_HOME`, si el
daemon está en marcha, si los cuatro binarios están juntos y si la carpeta está
en el PATH.

## Uso diario

```bash
hli                     # Agent Mode en el directorio actual
hli .                   # Agent Mode vinculado a esa ruta
hli "resume el repo"    # una pasada, sin TUI
hli -p "resume el repo" # igual, estilo `claude -p`
cat log.txt | hli -p "¿qué falla?"   # stdin como contexto
hli run "tarea"         # ejecución directa (alias de la anterior)
hli resume              # reanudar el último thread del directorio
hli inspect             # Inspector Mode
```

El daemon arranca solo la primera vez que se usa cualquier comando; no hay que
hacer `hli daemon start` a mano. El workspace, el thread y el branch del
directorio se crean implícitamente en la primera tarea.

## Opciones del instalador

| Windows | Unix | Efecto |
|---|---|---|
| `-Prefix DIR` | `--prefix DIR` | instala en `DIR` |
| `-NoPath` | `--no-path` | no modifica el PATH |
| `-SkipBuild` | `--skip-build` | usa `target/release` existente |
| `-Uninstall` | `--uninstall` | elimina binarios y la entrada del PATH |
| `-Source DIR` | — | raíz del repositorio |

## Datos y desinstalación

- Estado, eventos y artefactos: `HLI_HOME` (`~/.hli` por defecto).
- `--uninstall` solo quita binarios y PATH; `HLI_HOME` se conserva.

## Diferencias con la instalación de Claude Code

| Claude Code | HLI |
|---|---|
| Descarga un binario precompilado | Compila desde el repositorio (`cargo`) |
| Auto-actualización en background | `git pull && ./install.sh` |
| `claude update` | `git pull && ./install.sh --skip-build` tras compilar |
| `~/.local/bin/claude` | `~/.local/bin/hli` + tres binarios más en la misma carpeta |
