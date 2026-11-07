# HLI — Verificación con harness externo

**Fecha:** 2026-11-07 · **Alcance:** continuidad entre sesiones MCP y cambio de modelo.

## 1. un harness anterior: no evaluable en esta instalación

un harness anterior `0.2.1-rc.5` no expone una configuración MCP estable:

- `~/.dsh/profiles/desktop/cordis.yml` es una lista vacía; la composición se hace por
  `cordis.patch.yml` y bundles.
- No hay `mcpServers` ni sección MCP en el perfil.
- Añadir un plugin MCP implicaría tocar un harness congelado que ya sufrió una rotura por un
  bundle incompatible. **No se hizo.**

Por tanto, la integración con un harness anterior se declara **no evaluable**, no fallida. El gate se cubrió con
un cliente MCP externo real (proceso separado, JSON-RPC 2.0 con framing `Content-Length`), que es
el mismo protocolo que usaría un harness anterior.

## 2. Continuidad entre sesiones

`scripts/external_session.py` abre tres procesos MCP distintos contra el mismo `HLI_HOME`:

| Sesión | Acción | Resultado |
|---|---|---|
| A | `initialize` + `tools/list` + `memory_commit` de una decisión | 11 tools; decisión registrada |
| B | `initialize` + `memory_search` + run con artifact slice | encuentra la decisión; 1 slice; `payload_hash` |
| C | tras reiniciar el daemon, `memory_search` + `run_get` | encuentra la decisión y el run |

Evidencia literal:

```json
{
  "session_a_tools": ["workspace_list","thread_read","memory_search","memory_commit",
                      "run_start","run_get","artifact_read","module_list","mch_list",
                      "mch_inspect","mch_invoke"],
  "session_b_found": true,
  "session_c_found": true,
  "run_survives_restart": true,
  "run_slices": 1,
  "run_payload_hash": "sha256:7a4328359438765f5",
  "receipt_usage": 53,
  "handoffs": [
    {"from": null, "to": "sim", "model": "sim-1"},
    {"from": "sim", "to": "ollama", "model": "gemma4:31b-cloud"}
  ],
  "ok": true
}
```

## 3. Cambio de modelo

El mismo thread pasó de `sim` a `ollama` y emitió `provider.handoff` con `from`, `to` y `model`.
El workspace, la rama, la memoria y los runs anteriores se conservaron. No se mezclaron
credenciales: el segundo run no usó el secreto del primero (no había secreto).

## 4. Qué demuestra y qué no

**Demuestra:**

- MCH Memory y la memoria común funcionan desde un cliente MCP externo.
- La memoria sobrevive a cerrar sesión, abrir otra y reiniciar el daemon.
- Un run con artifact slice conserva manifiesto, `payload_hash`, receipt y usage.
- El cambio de proveedor conserva el estado y deja rastro.

**No demuestra:**

- Integración con un harness anterior (no evaluable por falta de MCP en su configuración).
- Que un harness comercial concreto se conecte sin adaptador; el cliente es el protocolo, no un
  producto.
- Reproducibilidad de un modelo real: el proveedor es determinista solo con temperatura 0, no
  por contrato.

## 5. Reproducción

```bash
cargo build --workspace
python scripts/external_session.py
```

Salida esperada: `{"ok": true, "session_b_found": true, "session_c_found": true, ...}`.
