# HLI — Informe de fase 0.4

**Fase:** HLI 0.4 — extensibilidad (Module SDK y MCH)
**Estado:** **NO INICIADA**
**Commit de referencia:** `1a484dd` (cierre de 0.2)
**Especificación:** §13 (Module SDK), §13.8 (MCH), §16.5
**Audiencia:** Copilot (auditor externo)

---

## 1. Aviso

Este informe **no documenta una implementación**. Fija el alcance y el gate de la fase. No hay
Module Protocol, ni paquetes `.hlimod`, ni MCH en el repositorio.

---

## 2. Alcance comprometido

| Componente | Contrato |
|---|---|
| Module Protocol | JSON-RPC 2.0 delimitado por línea sobre stdio; `module/discover`, `module/invoke`, `module/cancel`, `module/health`, `module/checkpoint`, `module/shutdown` |
| Sin handshake | El módulo está disponible cuando responde `module/discover` |
| Manifiesto | `apiVersion`, `kind`, `metadata`, `spec.sdk`, `capabilities`, `operations`, `permissions`, `interfaces` |
| Esquemas | Entrada/salida por operación, JSON Schema 2020-12 |
| Paquete `.hlimod` | Manifiesto, esquemas, binario/componente, firma, migraciones |
| Lockfile | `hli.lock` con versiones y hashes exactos |
| Pruebas de contrato | Instalar sin tocar el core; permisos no declarados fallan |
| Proyección | La misma operación como CLI y como tool MCP |
| MCH | Profile, Instance, Process, Endpoint sobre el mismo core |
| MCH Memory | Primer módulo de referencia |

**Fuera de 0.4:** marketplace, multiagente complejo, Desktop completo, sandbox multiplataforma
completo. El aislamiento por subproceso se implementa cuando una tool real lo exija.

---

## 3. Modelo de MCH que debe respetarse

```
HLI = runtime y harness completo
MCH = instancia especializada administrada por HLI
MCP = protocolo de interoperabilidad
CLI = interfaz textual
Module = implementación interna de una capacidad
```

Un MCH representa un **dominio limitado de capacidades**, no una única operación. Sus cuatro
piezas son distintas y deben existir por separado:

- **Profile**: declara dominio y módulos; no ejecuta.
- **Instance**: materializa el perfil en un workspace, con identidad y política.
- **Process**: el proceso que la hospeda (subproceso o servicio).
- **Endpoint**: la superficie publicada (MCP, CLI o ambas).

---

## 4. Gate propuesto

```
instalar módulo → validar manifiesto y firma
→ crear MCH (profile + instance)
→ exponerlo por CLI y por MCP
→ consumirlo desde otro harness
→ reiniciar HLI
→ recuperar módulo, instancia, memoria y estado
→ intentar un permiso no declarado y obtener denegación
```

Criterios de aceptación:

- [ ] `module.discover` responde sin `initialize`.
- [ ] El módulo no puede acceder a SQLite.
- [ ] Un permiso no declarado se rechaza con error accionable.
- [ ] La operación se proyecta igual en CLI y MCP.
- [ ] La instancia sobrevive al reinicio y conserva su estado.
- [ ] El paquete se instala sin ejecutar código antes de validar firma y permisos.

---

## 5. Riesgos previstos

| Riesgo | Mitigación |
|---|---|
| Sobreingeniería del protocolo de módulos | Solo seis métodos; sin estado de conexión |
| Módulo que se convierte en segundo servidor MCP | Prohibido en el contrato: el Core proyecta |
| Permisos ampliados por el módulo | La política efectiva es la del workspace; el módulo solo restringe |
| Estado propio de la instancia | Prohibido: el estado es el workspace |
| Paquete malicioso | Firma, inspección previa, sandbox cuando haya tool real |

---

## 6. Dependencias

- 0.2 cerrada: `Run`, `ContextManifest`, `Receipt`.
- 0.3 cerrada: MCP server y HLI Memory, porque MCH Memory se proyecta por MCP.

---

## 7. Lo que este informe no afirma

- No hay Module SDK.
- No hay MCH.
- No hay paquete `.hlimod`.
- No hay pruebas de contrato.
