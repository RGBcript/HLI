# HLI — Operations Handbook

**Versión:** `0.9.0-beta.1` · **Plataforma:** Windows x86_64 (Linux experimental)

Guía para operar HLI: instalar, diagnosticar, respaldar, migrar, recuperar,
exportar, firmar, configurar proveedores y administrar MCH. Todos los comandos
admiten `--output json` cuando se necesita automatizar.

> **Regla previa:** define siempre `HLI_HOME` de forma explícita. Si no existe la
> variable, el runtime usa `%USERPROFILE%\.hli`. El paquete y el instalador
> documentan `%LOCALAPPDATA%\HLI\data`. Mezclar ambos directorios es la causa más
> común de "he perdido mis datos".

---

## 1. Instalación

### 1.1 Desde el paquete

```powershell
cd dist\hli-0.9.0-beta.1-windows-x86_64
.\install.ps1
```

El instalador es **por usuario** (no requiere administrador). Verifica
`checksums.sha256`, copia binarios y documentación, crea el directorio de datos
sin secretos y añade HLI al PATH del usuario. No instala servicios ni tareas
programadas.

Comprobación inmediata:

```powershell
hli --version                      # hli 0.9.0-beta.1
$env:HLI_HOME = "$env:LOCALAPPDATA\HLI\data"
hli daemon start
hli daemon status
```

### 1.2 Desde el repositorio

```powershell
cargo build --workspace --release
target\release\hli.exe --version
```

Los binarios quedan en `target\release\`: `hli.exe`, `hli-daemon.exe`,
`hli-mcp.exe` y `hli-module-memory.exe`.

### 1.3 Desinstalación

```powershell
.\uninstall.ps1
```

Elimina binarios y la entrada de PATH. **Conserva `HLI_HOME`** por defecto: si
quieres borrar los datos, hazlo a mano y con copia previa.

---

## 2. Diagnóstico

Orden recomendado, de fuera hacia dentro:

| Paso | Comando | Qué responde |
|---|---|---|
| 1 | `hli --version` | ¿Qué binario estoy ejecutando? |
| 2 | `hli daemon status` | ¿Hay daemon, en qué PID y puerto, responde ping? |
| 3 | `hli status` | Estado agregado del almacén |
| 4 | `hli verify --workspace <ws>` | ¿La cadena de eventos sigue íntegra? |
| 5 | `hli migrate status` | ¿Esquema actual vs. esperado (v7)? |
| 6 | `hli module list` / `hli mch list` | ¿Módulos e instancias coherentes? |
| 7 | `hli mch inspect <name>` | Estado, PID, generación, reinicios |
| 8 | `hli daemon status` tras reiniciar | ¿Se recupera solo? |

Artefactos de diagnóstico: `daemon.json` (PID, puerto, versión), `daemon.log` y
`hli.lock`, todos en `HLI_HOME`.

Síntomas frecuentes:

| Síntoma | Causa probable | Acción |
|---|---|---|
| `daemon no está en marcha (falta daemon.json)` | Nunca arrancó o `HLI_HOME` distinto | `hli daemon start` con el `HLI_HOME` correcto |
| `daemon.json sin puerto` | Archivo truncado | Detener y arrancar de nuevo |
| Timeout en un comando | Daemon colgado o proveedor lento | Ver `daemon.log`; reiniciar daemon |
| Instancia MCH `failed` | Límite de reinicios agotado | `hli mch inspect` y `hli mch restart` |
| Run `failed` con error de proveedor | Red, clave o modelo inválido | `hli run get <id>` y revisar receipt |

---

## 3. Backups

Hay dos niveles y **no son intercambiables**:

1. **Backup del almacén (SQLite + migraciones)** — `hli migrate backup`. Es el
   mecanismo previsto antes de migrar.
2. **Copia del directorio `HLI_HOME`** — cubre además artefactos (`artifacts/`),
   módulos instalados (`modules/`), snapshots (`snapshots/`) y secretos
   (`secrets/`), que **no** están en la base.

Procedimiento recomendado antes de cualquier actualización:

```powershell
hli daemon stop
hli migrate backup
Copy-Item -Recurse $env:HLI_HOME "$env:HLI_HOME.bak-$(Get-Date -f yyyyMMdd-HHmm)"
hli daemon start
hli verify --workspace <ws>
```

Advertencia: un backup de `HLI_HOME` **con el daemon en marcha** puede capturar la
base a medio escribir. Detén el daemon o usa `migrate backup`, que está pensado
para eso.

---

## 4. Migraciones

El esquema se versiona por migraciones transaccionales. Versión actual: **7**.

```powershell
hli migrate status     # versión actual, objetivo y pendientes
hli migrate backup     # copia previa
hli migrate apply      # aplica pendientes, una transacción por paso
hli migrate verify     # comprueba integridad tras aplicar
```

Reglas operativas:

- **Nunca** editar `hli.db` a mano ni saltarse `migrate backup`.
- Si `apply` falla, la migración en curso se revierte (el gate de migraciones
  prueba el rollback ante una migración fallida).
- Tras migrar, ejecutar `hli verify --workspace <ws>` en los workspaces críticos.
- Un `import` sobre una base migrada debe venir de la misma versión de esquema.

---

## 5. Recovery

### 5.1 El daemon no arranca

1. `hli daemon stop` (ignora el error si no había daemon).
2. Revisar `daemon.log` y `daemon.json`.
3. Si `daemon.json` apunta a un PID que ya no existe, el arranque normaliza el
   estado efímero y republica el archivo.
4. Si el problema persiste, mover `daemon.json` y `hli.lock` a un lado (no
   borrarlos: son evidencia) y arrancar de nuevo.

### 5.2 La base no abre o `verify` falla

1. Detener el daemon.
2. Restaurar el último `migrate backup` o la copia de `HLI_HOME`.
3. `hli migrate status` y `hli migrate verify`.
4. `hli verify --workspace <ws>`.

`verify` detecta alteraciones en la cadena de eventos. **No** hay recuperación
automática de una cadena rota: se restaura copia y se documenta el incidente.

### 5.3 Instancias MCH

- `failed` por límite de reinicios → `hli mch restart <name>` (limpia la ventana,
  incrementa generación y ejecuta lifecycle completo).
- `degraded` → el proceso vive pero no responde health; `hli mch logs <name>` y
  `hli mch stop <name>` + `hli mch serve <name>`.
- Proceso huérfano tras un cierre sucio → `hli mch stop <name>` y verificar que
  el PID desapareció antes de volver a arrancar.

### 5.4 Terminal roto tras usar la TUI

La TUI instala un hook de pánico que restaura la pantalla. Si aun así queda en raw
mode: cerrar la pestaña del terminal y abrir una nueva. No hay comando `reset` en
HLI.

---

## 6. Export / Import

```powershell
hli export <workspace_id> --out copia.hliar
hli import copia.hliar
```

- El contenedor `.hliar` es **provisional** (ADR-0004): no es zip/zstd todavía.
- Exporta un workspace completo: eventos, ramas, runs, memorias y artefactos.
- El import **no limita tamaño** (limitación conocida): un paquete enorme consume
  memoria. Importar en una máquina con recursos suficientes.
- Tras importar: `hli migrate status` y `hli verify --workspace <ws>`.
- Los identificadores se conservan; importar dos veces el mismo paquete debe
  tratarse como operación idempotente a verificar, no asumirse.

---

## 7. Firmas de módulos

```powershell
hli key generate release-local
hli key list
hli key export-public <key_id> --out release.pub
hli trust add release.pub
hli module sign dist\hli-memory-0.9.0-beta.1.hlimod --key release-local
hli module verify dist\hli-memory-0.9.0-beta.1.hlimod
hli module install dist\hli-memory-0.9.0-beta.1.hlimod
```

Estados que reporta la instalación:

| Estado | Significado | Decisión |
|---|---|---|
| `signed-trusted` | Firma válida y clave en trust store | Instalar |
| `signed-untrusted` | Firma válida, clave desconocida | Añadir clave al trust o rechazar |
| `unsigned` | Sin firma | Solo con `warn-unsigned` explícito |

Reglas:

- La clave **privada** nunca sale del equipo ni se copia a `dist/`.
- `hli key revoke` invalida la clave; `hli trust revoke` la saca de confianza.
- La verificación de checksums del paquete es independiente de la firma: se
  comprueban ambas.

---

## 8. Providers

### 8.1 Usar uno en un run

```powershell
hli run start --workspace $ws --thread $th --branch $br `
  --prompt "resume el estado" --provider sim

hli run start ... --provider ollama --model llama3 --base-url http://127.0.0.1:11434

hli run start ... --provider openai --model gpt-4o-mini `
  --base-url https://api.openai.com/v1 --secret openai
```

### 8.2 Secretos

El valor **no** se pasa por línea de comandos ni se guarda en la base: se guarda
en `HLI_HOME\secrets\<handle>` y el comando recibe `--secret <handle>`. El run
solo referencia el handle.

### 8.3 Administración

```powershell
hli provider list
hli provider add <name> --kind ollama|openai --base-url <url> --model <m>
hli provider inspect <name>
hli provider set-default <name>
hli provider test <name>
hli provider remove <name>
hli prices list
```

Notas:

- Sin conexión, `sim` es el único proveedor útil; los gates lo usan por eso.
- El coste se calcula con `prices.yaml`; si falta el precio, el coste es 0 y el
  receipt lo refleja así.
- No hay streaming: la respuesta llega completa o falla por timeout.

---

## 9. MCH

### 9.1 Ciclo de vida

```powershell
hli mch create memory-main --module hli-memory --domain memory --workspace $ws
hli mch create memory-main ... --autostart      # arranca al iniciar el daemon
hli mch serve memory-main                       # arranque explícito
hli mch inspect memory-main                     # state, pid, process_generation, reinicios
hli mch status memory-main
hli mch logs memory-main
hli mch stop memory-main
hli mch restart memory-main
hli mch remove memory-main
hli mch set-autostart memory-main true|false
hli mch list
```

Estados: `starting`, `ready`, `degraded`, `failed`, `stopped`.

### 9.2 Política de reinicios

- Máximo **3 reinicios** dentro de la ventana configurada.
- El cuarto intento deja `failed` y **no** crea proceso nuevo.
- Una instancia `failed` por límite **no** se reinicia sola. Requiere
  `hli mch restart`, que limpia la ventana, incrementa `process_generation` y
  ejecuta lifecycle completo.
- Al arrancar el daemon: abre store → normaliza PIDs y estados → publica
  `daemon.json` → acepta conexiones → arranca autostarts en background. Un
  autostart fallido **no** bloquea el daemon.

### 9.3 Invocar operaciones

```powershell
hli mch invoke memory-main memory.search --args '{"query":"fuente"}'
hli mch invoke memory-main memory.commit --args '{"type":"fact","content":"..."}'
```

Las operaciones válidas son las que declara el módulo (para `hli-memory`:
`memory.commit`, `memory.get`, `memory.search`, `memory.update`,
`memory.forget`, `context.build`). Una operación no declarada se rechaza.

---

## 10. Rutina mínima de operación

**Diaria**

```powershell
hli daemon status
hli mch list
```

**Semanal**

```powershell
hli verify --workspace <ws>
hli migrate status
```

**Antes de actualizar**

```powershell
hli daemon stop
hli migrate backup
# copia de HLI_HOME
hli daemon start
hli migrate apply
hli verify --workspace <ws>
```

**Antes de publicar un módulo**

```powershell
hli module verify <paquete.hlimod>
hli module test <nombre>
```

---

## 11. Límites operativos que conviene recordar

- Sin autenticación remota: el daemon escucha en loopback. No exponerlo.
- Sin sandbox: `shell.execute` está denegado; no hay aislamiento de CPU/memoria.
- Sin límite de tamaño en `import` ni en `read_container`.
- Sin streaming ni reconexión automática de la TUI: si el daemon cae, hay que
  reabrirla.
- Sin scheduler: HLI no ejecuta tareas periódicas por sí solo.

---

## Aprobar un run con efectos

```bash
hli run plan <run_id>                       # pasos, efectos y hashes
hli run approve <run_id> --plan <plan_id> --scope once|run|workspace
hli run reject  <run_id> --plan <plan_id> --reason "motivo"
hli run replan  <run_id> [--reason "motivo"]
```

Diagnóstico: `hli event list --workspace <id>` muestra `plan.created`,
`run.waiting_approval`, `plan.approved|rejected|invalidated|revised` y el resto
de la traza. Un run `waiting_approval` sobrevive al reinicio del daemon.

---

## Decidir un permiso

```bash
hli permission list --run <run_id> --status pending
hli permission inspect <request_id>
hli permission approve <request_id> --scope once|run|workspace
hli permission deny <request_id> --reason "motivo"
hli permission cancel <request_id>
```

Un run en `waiting_approval` con `approval_kind=tool` conserva la solicitud
aunque se reinicie el daemon. Aprobar ejecuta una sola vez; repetir la misma
decisión es idempotente.
