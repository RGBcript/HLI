# HLI — Informe de fase 0.4

**Fase:** HLI 0.4 — extensibilidad (Module SDK y MCH)
**Estado:** cerrada y verificada por gate externo
**Especificación:** §13, §13.8, §16.5
**Audiencia:** Copilot (auditor externo)
**Fecha:** 2026-11-07

---

## 1. Resumen ejecutivo

0.4 cierra el roadmap: Module Protocol propio, paquete `.hlimod` con checksums, supervisor
de subproceso, proyección única a CLI y MCP, y MCH Memory como primer dominio.

**Demostrado con `scripts/module_gate.py`**, un gate externo que empaqueta el módulo, lo
instala, ejecuta pruebas de contrato, crea MCH Profile e Instance, arranca el proceso, invoca
por CLI y por MCP, bloquea una operación no declarada, reinicia el daemon, exporta e importa
el workspace y verifica la memoria.

**No implementado en 0.4:** firma criptográfica del paquete, pool persistente de procesos,
WASI, sandbox de sistema operativo, ACP, Desktop y embeddings.

---

## 2. Module Protocol

JSON-RPC 2.0 delimitado por línea sobre stdio, separado de MCP.

| Método | Función |
|---|---|
| `initialize` | identidad, versión y operaciones |
| `capabilities` | operaciones declaradas |
| `configure` | configuración de la instancia |
| `invoke` | ejecuta una operación |
| `cancel` | cancelación cooperativa |
| `health` | sonda de vida |
| `checkpoint` | punto de control (sin estado propio) |
| `shutdown` | cierre ordenado |

Cada request lleva `_meta` con `hli.dev/workspaceId`, `requestId` e `idempotencyKey`.
El módulo `hli-memory` no abre `hli.db`, no lee secretos, no expone MCP y no escribe fuera
del workspace: delega en el daemon por TCP.

---

## 3. Paquete `.hlimod`

Contenedor `HLIAR1` con:

```
manifest.json
schemas/*.json
bin/hli-memory.exe
checksums.json
```

`manifest.json` declara nombre, versión, rango de SDK, operaciones con esquemas de entrada y
salida, efectos, permisos, runtime y lifecycle. Se eligió JSON en lugar de YAML para no añadir
dependencia (ADR-0008).

`hli module install` verifica:
- que existe `manifest.json` y `checksums.json`;
- que el rango de SDK contiene `0.4`;
- que cada archivo coincide con su checksum.

Después copia a `$HLI_HOME/modules/<name>/<version>/` y escribe `hli.lock`.

Comandos: `module install|list|inspect|test|remove`.

---

## 4. Supervisor

- Subproceso aislado por protocolo; sin `libloading` ni carga dinámica.
- Timeout de 30 s con `kill` al vencer.
- Límite de stderr capturado (64 KiB) y de artefacto (1 MiB por respuesta).
- Health check por `initialize` + operación de sonda.
- Reinicio controlado: una invocación nueva por llamada; si falla, se reporta.
- Operación no declarada: el daemon la rechaza antes de lanzar el proceso.

Deuda: no hay pool persistente; `mch.serve` valida y deja la instancia en `ready`, pero cada
`invoke` usa un proceso nuevo.

---

## 5. Proyección

Una operación del manifiesto se expone:

| Superficie | Ejemplo |
|---|---|
| CLI | `hli mch invoke memory-main memory.search --args '{"query":"MCP"}'` |
| MCP | `mch_invoke {name, operation, arguments}` |
| Recurso MCP | `hli://mch/memory-main` |

La validación de operaciones declaradas vive en el daemon; CLI y MCP comparten el mismo
esquema de error (`undeclared_operation`).

---

## 6. MCH

| Entidad | Dónde |
|---|---|
| Profile | `mch_profiles`: nombre, módulo, dominio, manifiesto |
| Instance | `mch_instances`: perfil, workspace, estado, pid |
| Process | subproceso lanzado por el supervisor |
| Endpoint | CLI y MCP |

Comandos: `mch create|list|inspect|serve|stop|remove`.

MCH Memory se crea con `--module hli-memory --domain memory`; reutiliza `memories` y
`memories_fts`, no una segunda base.

---

## 7. Artefactos binarios

`artifact.read` devuelve:

```json
{"id":"art_...","sha256":"sha256:...","mime":"image/png","bytes":1234,
 "encoding":"hex","content":"89504e47..."}
```

- Texto UTF-8 → `encoding: "utf8"`.
- Binario → `encoding: "hex"`.
- Mayor de 1 MiB → `encoding: "too-large"`, sin contenido.

---

## 8. Gate 0.4: evidencia

```
$ python scripts/module_gate.py
{"gate": "0.4", "ok": true, "module": "hli-memory",
 "instance": "memory-main", "mcps": ["mch_invoke", "mch_list"], "memories": 1}
```

El gate cubre:

1. empaquetar `hli-memory.hlimod`;
2. verificar checksums;
3. instalar;
4. `module test`;
5. crear Profile e Instance `memory-main`;
6. `mch serve` → `ready`;
7. invocar `memory.commit` por CLI;
8. invocar `memory.search` por CLI;
9. descubrir `mch_invoke` y `mch_list` por MCP;
10. registrar y recuperar memoria por MCP;
11. operación no declarada rechazada;
12. vigencia (`valid_until`) aplicada;
13. checksum manipulado rechazado;
14. SDK incompatible rechazado;
15. reinicio del daemon con módulo, MCH y memoria intactos;
16. export/import con memoria equivalente.

---

## 9. Huecos y riesgos

### Alta

1. **Sin firma criptográfica.** `checksums.json` detecta corrupción, no suplantación. Un
   paquete modificado puede recalcular sus checksums.
2. **Sin sandbox de SO.** El supervisor aísla por protocolo y permisos, no por kernel. Un
   módulo nativo malicioso puede hacer lo que permita el usuario.

### Media

3. **Supervisor efímero.** Sin pool, cada invocación paga el arranque del proceso.
4. **Sin WASI.** Solo subproceso nativo.
5. **`skills`/`migrations` no se aplican.** El manifiesto las contempla, el instalador no las
   ejecuta.
6. **MCP sin `tasks`.** `mch_invoke` es síncrono.

### Baja

7. `hli.lock` se reescribe completo en cada install.
8. `module.remove` borra el directorio del módulo, no las instancias MCH que lo usan.
9. `mch.serve` no mantiene el proceso vivo.

---

## 10. Guía de auditoría

### Verificar

1. El módulo no contiene llamadas a SQLite; solo TCP al daemon.
2. `module.install` verifica checksums antes de copiar.
3. El rango de SDK se comprueba antes de instalar.
4. `mch.invoke` rechaza operaciones que no están en el manifiesto.
5. CLI y MCP usan el mismo comando del daemon.
6. `memory.search` filtra `valid_from` y `valid_until`.
7. `MCH` usa tablas propias y no duplica memorias.
8. `artifact.read` limita tamaño y no inyecta binario en texto.
9. El reinicio conserva módulo, instancia y memoria.
10. Export/import reindexa FTS.

### Preguntas

- ¿Un módulo puede abrir `hli.db`? (Técnicamente sí, si el usuario le da permisos de archivo;
  el contrato se lo prohíbe y el código de referencia no lo hace. Falta sandbox de SO.)
- ¿El checksum cubre el manifiesto? (Sí, está en `checksums.json`.)
- ¿Se puede instalar dos versiones del mismo módulo? (Sí, la clave es `(name, version)`.)
- ¿MCH es un servidor MCP? (No: es una instancia que se proyecta por CLI y MCP.)

### Lo que este informe NO afirma

- No afirma firma ni cadena de confianza del paquete.
- No afirma aislamiento de sistema operativo.
- No afirma WASI.
- No afirma que `cargo test` cubra 0.4: el gate es `scripts/module_gate.py`.

---

## 11. Estado

- `cargo test`: 27 pruebas en verde.
- Gates externos: `scripts/mcp_gate.py` (0.3) y `scripts/module_gate.py` (0.4).
- Matriz viva: `docs/CONFORMANCE.md`.
- Roadmap 0.1–0.4 completo.
