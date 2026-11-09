# HLI 0.9.0-beta.1 — Quickstart

Objetivo: de cero a memoria persistente en menos de 15 minutos, usando **solo el
paquete instalado** (no `cargo run`).

## 0. Requisitos

- Windows 10/11 x86_64.
- PowerShell 5.1 o superior.
- Opcional: Ollama local para un modelo real.

## 1. Instalar

Desde la carpeta `hli-0.9.0-beta.1-windows-x86_64`:

```powershell
.\install.ps1
```

Instala en `%LOCALAPPDATA%\HLI\bin` y datos en `%LOCALAPPDATA%\HLI\data`, sin
administrador ni secretos. Si no quieres tocar el PATH:

```powershell
.\install.ps1 -NoPath
```

En cada sesión nueva:

```powershell
$env:HLI_HOME = "$env:LOCALAPPDATA\HLI\data"
$env:Path = "$env:LOCALAPPDATA\HLI\bin;$env:Path"
hli --version
hli                       # Agent Mode en el proyecto actual
hli resume                # selector de threads recientes
```

Debe imprimir `hli 0.9.0-beta.1`.

## 2. Arrancar el daemon

```powershell
hli daemon start
hli daemon status
```

## 3. Workspace, hilo y rama

```powershell
hli workspace create quickstart
```

Copia el `id` devuelto (`ws_...`) y úsalo en los siguientes comandos:

```powershell
hli thread create "primer hilo" --workspace ws_...
hli branch create main --thread thr_...
```

## 4. Ejecutar con SimProvider

```powershell
hli provider add sim1 --kind sim --base-url http://localhost --model sim-1
hli provider set-default sim1
hli run start --workspace ws_... --thread thr_... --branch br_... --prompt "hola HLI"
```

`SimProvider` **no representa un modelo real**: genera una respuesta simulada
para probar el runtime sin red ni claves.

## 5. TUI

```powershell
hli
```

La TUI abre con el `HLI_HOME` actual. Para el gate visual manual sigue
`docs/TUI-MANUAL-GATE.md`.

## 6. Módulo y MCH Memory

```powershell
hli trust add .\release-public.json
hli module install .\hli-memory-0.9.0-beta.1.hlimod
hli mch create memory-main --module hli-memory --workspace ws_...
hli mch serve memory-main
```

Registrar y consultar memoria:

```powershell
hli memory commit --workspace ws_... --type fact --content "El límite es 100."
hli memory search --workspace ws_... --query "límite"
hli mch invoke memory-main memory.search --args '{"query":"límite"}'
```

## 7. Persistencia

```powershell
hli daemon stop
hli daemon start
hli memory search --workspace ws_... --query "límite"
```

La memoria debe seguir ahí.

## 8. MCP

El binario `hli-mcp.exe` habla JSON-RPC 2.0 por stdio con framing
`Content-Length`. Configura tu cliente MCP con:

```
command: %LOCALAPPDATA%\HLI\bin\hli-mcp.exe
env: HLI_HOME=%LOCALAPPDATA%\HLI\data
```

Herramientas expuestas: `workspace_list`, `thread_read`, `memory_search`,
`memory_commit`, `run_start`, `run_get`, `artifact_read`, `module_list`,
`mch_list`, `mch_inspect`, `mch_invoke`. Ver `docs/MCP-GUIDE.md`.

## 9. Ollama opcional

Con un servidor Ollama en `http://127.0.0.1:11434`:

```powershell
hli provider add local --kind ollama --base-url http://127.0.0.1:11434 --model llama3
hli run start --workspace ws_... --thread thr_... --branch br_... --prompt "hola" --provider local
```

OpenAI-compatible requiere `secret_handle`; el secreto vive en
`HLI_HOME/secrets/<handle>` o en una variable `HLI_SECRET_<HANDLE>`.

## Límites honestos

- `shell.execute` permanece **denegado**.
- No hay sandbox completo ni shell seguro.
- SimProvider no es un modelo real.
- Ollama requiere un servidor disponible.
- Soporte Linux experimental; el paquete probado es Windows x86_64.

---

## Aprobar una tarea con efectos

```bash
hli run "corrige el archivo notas.txt"
# → waiting_approval
hli run plan <run_id>        # revisa pasos, efectos y hashes
hli run approve <run_id> --plan <plan_id> --scope once
hli run status <run_id>      # → completed y receipt con procedencia
```

Si el proveedor o el modelo cambian entre el plan y la aprobación, HLI invalida
la aprobación y exige `hli run replan <run_id>` antes de ejecutar.
