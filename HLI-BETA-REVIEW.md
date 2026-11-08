# HLI — Beta Readiness Review

**Versión evaluada:** `0.9.0-beta.1` · **Commit:** `6f273be` · **Fecha:** 2026-11-07
**Veredicto:** **Beta funcional, no publicable todavía.**
El paquete existe y los gates automáticos están verdes; el tag está bloqueado por
un gate manual cuyo propio guion no coincide con la TUI que debe verificar.

Este documento no vende nada. Lo que no se probó, dice que no se probó.

---

## 1. Implementado

| Área | Qué existe | Evidencia |
|---|---|---|
| Runtime | Event log con cadena SHA-256, snapshots, export/import | `hli-store`, gate de migraciones y MCP |
| Runs | Síncronos y asíncronos, cancelación, timeout, aprobación | `hli-daemon`, 22 pruebas |
| Contexto | Manifiesto inmutable previo a la llamada, con exclusiones y hash | ADR-0006, `hli-agent` |
| Memoria | 6 tipos, 4 ámbitos, supersesión, autoridad, estados | `hli-store` (23 pruebas), pestaña Memory |
| Artefactos | Almacenamiento por SHA-256, MIME, slices con motivo | `hli-store`, gate MCP |
| Proveedores | `sim`, Ollama y OpenAI-compatible; precios y coste | `hli-provider` |
| MCH | Lifecycle, health, autostart, límite de 3 reinicios, generación | gate del supervisor |
| Firma | Ed25519, trust store, políticas, tamper y revocación | gate de firma, 12 pruebas |
| Migraciones | Esquema v7, 7 migraciones, backup, rollback | gate de migraciones |
| Distribución | Paquete Windows, instalador/desinstalador por usuario, 53 archivos, checksums | gate de instalación |
| Superficies | CLI (~30 comandos), TUI (6 pestañas), MCP (11 tools) | pruebas y gates |
| CI | Windows completo; Linux núcleo | `.github/workflows/ci.yml` |

Cifras: **134 pruebas** en debug y release, 0 fallos; `cargo audit` 0
vulnerabilidades y 3 avisos; `cargo deny check` en verde con avisos justificados.

---

## 2. Parcial

| Área | Qué hay | Qué le falta |
|---|---|---|
| **TUI** | Funciona y se renderiza correctamente en PTY a 120×30 | Gate manual sin ejecutar ni firmar; guion desalineado; sin streaming |
| **MCP** | 11 tools, framing correcto, cliente externo propio | Recursos y prompts vacíos; sin transporte HTTP; sin probar con clientes de terceros |
| **Linux** | Compila y pasa el núcleo | Gates de integración sin portar; nadie lo ejecuta como plataforma |
| **Firma** | Cadena local verificable | Sin PKI, sin revocación global, sin transparencia de claves |
| **Migraciones** | Aplican y revierten con gate | No hay restauración automática ni prueba de salto entre versiones reales |
| **MCH** | Supervisor sólido en lo probado | Sin sandbox, sin cuotas de CPU/memoria, sin límite de instancias |
| **Proveedores** | Tres adaptadores y cálculo de coste | Sin streaming, sin reintentos configurables, precios manuales |
| **Backups** | `migrate backup` y copia de `HLI_HOME` | Sin verificación de backup restaurado en el flujo normal |
| **Documentación** | 57 documentos | Deriva comprobada entre guion y código (ver §5) |

---

## 3. Pendiente

**Bloquea el tag**

1. Ejecutar y firmar los 19 pasos de `docs/TUI-MANUAL-GATE.md`.
2. Corregir ese guion: pasos 4, 5 y 13 no describen la TUI real (verificado por
   PTY el 2026-11-07).
3. Hacer que `beta_gate.ps1` derive `tui_manual` de la firma, no de una constante
   `$false`.

**No bloquea el tag, pero bloquea afirmaciones**

4. Portar gates de integración a Linux.
5. Probar con un cliente MCP de terceros.
6. SBOM y revisión de dependencias más allá de RustSec.
7. ADRs de firma, migraciones, empaquetado y autostart.
8. Sandbox o eliminación definitiva de `shell.execute`.
9. Routing económico con medición.
10. Streaming en la TUI; preferencias TOML; `Alt+1..6`.

---

## 4. No probado

Esto **no** está cubierto por ninguna prueba ni gate, y no debe afirmarse:

| Hipótesis sin verificar | Por qué importa |
|---|---|
| Corte de energía o `kill -9` a mitad de escritura | Consistencia real de SQLite + event log |
| Importación de un `.hliar` corrupto o gigante | `import` no limita tamaño |
| Concurrencia de varios clientes escribiendo a la vez | Solo el daemon escribe, pero no se probó carga |
| MCH durante horas o días | Fugas de handles, deriva de PID, salud a largo plazo |
| Disco lleno o permisos denegados | Comportamiento ante `ENOSPC`/`EACCES` |
| Rutas con Unicode o espacios en `HLI_HOME` | Windows real, no solo rutas de prueba |
| Actualización desde una base 0.8 real | El gate usa fixture, no un salto de versión histórico |
| Antivirus / SmartScreen bloqueando el instalador | Distribución real en equipos de terceros |
| Clientes MCP de terceros (Claude Desktop, IDEs) | Compatibilidad MCP universal no demostrada |
| La instalación de otro usuario sin privilegios | El gate corre en el equipo del desarrollador |

---

## 5. Riesgos

| # | Riesgo | Prob. | Impacto | Mitigación actual | Estado |
|---|---|---|---|---|---|
| R1 | Crear el tag con `ok=false` y declarar una beta inexistente | Media | Alto | El gate lo impide; el tag no se creó | Controlado |
| R2 | **Deriva documentación↔código**: el gate manual describe una TUI que no existe | **Confirmada** | Alto | Ninguna; detectada en esta revisión | **Abierto** |
| R3 | `lru 0.12.5` con dos `unsound` vía ratatui | Baja | Medio | No alcanzable desde HLI; justificado en `deny.toml` | Aceptado |
| R4 | `paste` sin mantenimiento | Baja | Bajo | Solo en compilación de ratatui | Aceptado |
| R5 | `artifact.put` no confina la ruta de origen | Media | Medio | Operación local del usuario | Aceptado |
| R6 | `import`/`read_container` sin límite de tamaño | Media | Alto (OOM) | Ninguna | **Abierto** |
| R7 | Monolito `hli-daemon/main.rs` (2.532 líneas) | Alta | Medio | Ninguna | Deuda |
| R8 | Sin sandbox; `shell.execute` denegado por diseño | Baja | Alto si se habilita | Denegación explícita | Aceptado |
| R9 | Secretos legibles por cualquiera con acceso al FS | Media | Medio | Solo `secrets/` o entorno | Aceptado |
| R10 | MCP sin autenticación | Baja | Medio | Solo loopback y stdio | Aceptado |
| R11 | Ausencia de pruebas en `hli-mcp` y módulos | Alta | Medio | Gates externos | Deuda |

---

## 6. Deuda técnica

Ordenada por coste de no pagarla:

1. **Guion del gate visual incorrecto.** Es el bloqueo actual y además erosiona la
   confianza en el resto de la documentación. Coste: horas. Riesgo de no hacerlo:
   el gate no se puede firmar honestamente.
2. **`tui_manual` como constante.** Un gate que no puede pasar por diseño no es un
   gate: es una nota al pie. Debe leer la firma o un archivo de estado.
3. **MCP y módulos sin pruebas unitarias.** Toda su cobertura es externa y lenta;
   322 + 186 líneas sin un solo `#[test]`.
4. **Daemon monolítico.** 2.532 líneas con despacho, supervisión, proveedores y
   validaciones mezcladas. Extraer `commands/`, `mch/`, `providers/`.
5. **Cuatro decisiones sin ADR** (firma, migraciones, empaquetado, autostart).
6. **Linux de segunda clase.** CI parcial y gates no portados: o se soporta o se
   declara no soportado, sin ambigüedad.
7. **Deriva de docs en general.** 57 documentos sin verificación de vigencia; el
   caso de la TUI demuestra que ocurre.
8. **Sin SBOM.** `cargo deny` cubre avisos, no procedencia.
9. **Pruebas de robustez ausentes** (caída, disco lleno, import corrupto).

---

## 7. Qué falta para decir "Beta pública"

Condiciones mínimas y verificables:

- [ ] Gate visual ejecutado en Windows Terminal y **firmado** por el usuario.
- [ ] Guion del gate corregido o código ajustado; los 19 pasos pasan sin
      interpretación.
- [ ] `beta_gate.ps1` devuelve `ok=true` con la firma como entrada.
- [ ] Tag `v0.9.0-beta.1` creado y paquete publicado con checksums.
- [ ] Limitaciones publicadas tal cual (Linux experimental, sin sandbox, MCP
      básico).
- [ ] Al menos una prueba de import corrupto y una de corte durante escritura, o
      declararlas explícitamente como no soportadas.

## 8. Qué falta para decir "1.0"

- Sandbox o decisión formal de no tenerlo.
- Linux soportado o retirado.
- Cliente MCP de terceros validado.
- SBOM y política de dependencias.
- ADRs completos.
- Pruebas de robustez y concurrencia.
- Superficie de API congelada y versionada.

---

## 9. Conclusión

HLI 0.9 es un sistema **coherente y verificable en lo automático**: el event log
se puede auditar, la memoria tiene historia, el supervisor MCH aguanta lo que el
gate le exige y el paquete se instala limpio.

Lo que impide llamarlo beta pública no es código: es **verificación pendiente y
un documento que miente sin querer**. El gate visual describe una interfaz que no
es la que se implementó; firmarlo tal cual sería firmar una ficción, y corregirlo
sin ejecutarlo sería lo mismo con otro formato.

El orden correcto es: corregir el guion, ejecutarlo, firmar, ajustar el gate,
tag. Nada más. Todo lo demás de esta lista puede esperar a 1.0 sin vergüenza.
