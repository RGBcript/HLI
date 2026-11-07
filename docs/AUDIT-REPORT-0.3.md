# HLI — Informe de fase 0.3

**Fase:** HLI 0.3 — interoperabilidad (MCP y HLI Memory)
**Estado:** **NO INICIADA**
**Commit de referencia:** `1a484dd` (cierre de 0.2)
**Especificación:** §11 (MCP), §14 (MCH Memory), §16.4
**Audiencia:** Copilot (auditor externo)

---

## 1. Aviso

Este informe **no documenta una implementación**. Documenta el alcance cerrado de la fase, su
gate y sus criterios de aceptación, para que el auditor sepa qué debe existir cuando se abra el
bloque. No hay código MCP ni memoria en el repositorio; no hay stubs que aparenten lo contrario.

---

## 2. Alcance comprometido

| Componente | Contrato |
|---|---|
| MCP Host | Cliente para servidores externos; credenciales, permisos y límites por servidor |
| MCP Server | `server/discover`, `tools/list`, `tools/call`, `resources/list`, `resources/read`, `prompts/list` |
| Tools | `workspace.list`, `thread.read`, `memory.search`, `run.start`, `run.get`, `artifact.read` |
| Resources | `hli://workspace/{id}`, `hli://thread/{id}`, `hli://artifact/{id}`, `hli://run/{id}` |
| Prompts | Plantillas por perfil: `hli.plan`, `hli.review` |
| HLI Memory | `memory.commit`, `memory.search`, `memory.get`; tipos, ámbitos, estados |
| Recuperación | FTS5/BM25 determinista; `query_hash` en el manifiesto |
| Export | Incluir `runs` y `context_manifests` en el archivo `hliar` |

**Fuera de 0.3:** ACP y compatibilidad MCP legacy. Son experimentales y no bloquean el gate.

---

## 3. Decisiones de diseño ya tomadas

1. **MCP 2026-07-28 como baseline**, no como revisión permanente. El servidor declara la versión
   en `_meta`; el cliente detecta era solo si se implementa el camino legacy (no en 0.3).
2. **Sin sesiones de protocolo.** Cada request lleva `workspace_id` y los handles explícitos.
   No hay `Mcp-Session-Id`.
3. **Tools y resources separados.** Una lectura no se expone como tool con efectos.
4. **Memoria con procedencia.** Cada memoria guarda `source_events`, `supersedes`, `contradicts`
   y `sensitivity`; `memory.search` devuelve la fuente.
5. **La proyección conserva el esquema.** La misma operación se expone en CLI y en MCP con el
   mismo JSON Schema.

---

## 4. Gate propuesto

Un cliente MCP externo debe:

```
conectar → server/discover → tools/list
→ workspace.list → thread.read
→ memory.search
→ run.start o run.get
→ artifact.read
→ cerrar
→ reconectar sin perder estado
```

Criterios de aceptación:

- [ ] El cliente descubre capacidades sin handshake previo.
- [ ] `tools/call` de `run.start` devuelve un `run_id` consultable.
- [ ] `memory.search` devuelve resultados con `source_events`.
- [ ] `artifact.read` devuelve el contenido por hash.
- [ ] Tras reconectar, el estado es idéntico.
- [ ] Una tool con `effects: destructive` no se expone sin política explícita.

---

## 5. Riesgos previstos

| Riesgo | Mitigación |
|---|---|
| Prometer conformidad MCP sin prueba | Gate con cliente real; matriz de conformidad actualizada |
| FTS5 con tokenización incorrecta en español | `unicode61 remove_diacritics 2` + trigram; pruebas con corpus |
| Memoria sin write-path definido | `memory.commit` explícito; sin extracción automática en 0.3 |
| Export incompleto | Añadir `runs` y `manifests` al contenedor antes de cerrar el gate |
| Tool poisoning en host | Resultados externos delimitados como datos, nunca instrucciones |

---

## 6. Dependencias

- 0.2 cerrada (`1a484dd`): `Run`, `ContextManifest`, `Receipt`, Tool Runtime, permisos.
- SQLite FTS5 disponible en `rusqlite` (bundled).
- Ninguna dependencia externa de red para el gate: el cliente MCP de prueba habla stdio.

---

## 7. Lo que este informe no afirma

- No hay servidor MCP.
- No hay memoria.
- No hay cliente MCP probado.
- No hay conformidad declarada con ninguna revisión de MCP.
