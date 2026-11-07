# HLI — Firma opcional de módulos (Ed25519)

**Estado:** implementado. Commit inicial del bloque: `43eef97`.
Commit final del bloque: `feat(module): add optional Ed25519 signing`.

## Qué se firma

No se firma el contenedor `.hlimod` crudo: el orden de secciones y metadatos
puede variar sin cambiar el contenido lógico. Se firma un **manifiesto canónico**
de archivos:

```json
{
  "format": "hli-signing/v1",
  "files": [
    {"path": "bin/app.bin", "sha256": "<hex>", "size": 1234},
    {"path": "checksums.json", "sha256": "<hex>", "size": 456},
    {"path": "manifest.json", "sha256": "<hex>", "size": 789}
  ],
  "migrations": [{"id": "0001", "path": "migrations/0001.sql"}]
}
```

- Archivos ordenados por ruta (bytes) y sin `signature.json`.
- Se incluyen `manifest.json`, `checksums.json`, esquemas, binario/componentes y
  cualquier otro archivo del paquete.
- `migrations` reproduce las migraciones declaradas en el manifest.
- El JSON se serializa con la canonicalización RFC 8785 de HLI.
- `payload_hash = "sha256:" + sha256(bytes canónicos)`.
- La firma Ed25519 cubre los bytes canónicos completos (no solo el hash).

## signature.json

```json
{
  "algorithm": "ed25519",
  "key_id": "key_0123456789abcdef",
  "public_key_fingerprint": "sha256:...",
  "signed_at": "2026-11-07T21:00:00.000Z",
  "payload_hash": "sha256:...",
  "signature": "base64:..."
}
```

La firma es determinista para el mismo payload y la misma clave (RFC 8032), por
lo que dos firmas del mismo contenido producen la misma `signature`.

## Estados

| Estado | Significado |
|---|---|
| `unsigned` | no hay `signature.json` |
| `signed-untrusted` | firma presente y payload coherente, pero la clave no está en el trust store |
| `signed-trusted` | firma válida, clave confiada y no revocada |
| `signed-revoked` | firma válida, pero la clave fue revocada localmente o en el trust store |
| `invalid-signature` | payload alterado, fingerprint incorrecto, firma inválida o algoritmo no soportado |

## Políticas

`HLI_HOME/module-policy.json`:

```json
{"policy": "warn-unsigned"}
```

`HLI_MODULE_POLICY` tiene prioridad (útil para gates y pruebas). Default:
`warn-unsigned`.

| Estado | allow-unsigned | warn-unsigned | require-signed |
|---|---|---|---|
| `unsigned` | acepta | acepta + warning | rechaza |
| `signed-untrusted` | acepta + warning | acepta + warning | rechaza |
| `signed-trusted` | acepta | acepta | acepta |
| `signed-revoked` | rechaza | rechaza | rechaza |
| `invalid-signature` | rechaza | rechaza | rechaza |

La política se evalúa en `module.install` **antes** de copiar archivos o
ejecutar el módulo. Una firma inválida o una clave revocada se rechazan con
código `signature_rejected` en cualquier política.

## Comandos

```powershell
hli key generate <name>
hli key list
hli key inspect <key_id>
hli key export-public <key_id>
hli key revoke <key_id>

hli trust add <public-key-file>
hli trust list
hli trust remove <key_id>
hli trust revoke <key_id>

hli module sign <package.hlimod> --key <key_id>
hli module verify <package.hlimod>
hli module install <package.hlimod>
```

`module verify` valida límites y rutas del contenedor, `checksums.json`, payload
canónico, firma Ed25519, fingerprint, confianza y revocación. Devuelve el estado
y el resultado de la política (`accepted`, `warning`).

## Marca de módulos ya instalados

`module.list` e `module.inspect` recalculan el estado de firma desde los
archivos instalados. Revocar una clave no desinstala ni detiene el módulo: el
listado pasa a mostrarlo como `signed-revoked`, pero el proceso y su estado
siguen intactos.

## Verificación

| Comprobación | Dónde |
|---|---|
| 14 pruebas obligatorias | `crates/hli-signing/tests/signing.rs` (12) + `crates/hli-cli/tests/signing.rs` (2) |
| Gate externo | `scripts/module_signing_gate.py` |
| Gate 0.4 existente | sigue aceptando paquetes sin firma bajo `warn-unsigned` |

Salida del gate:

```json
{
  "gate": "module-signing",
  "ok": true,
  "signed": true,
  "trusted": true,
  "tamper_rejected": true,
  "revocation_enforced": true,
  "private_key_leak": false
}
```

## Limitaciones

- No hay PKI, certificados X.509 ni servicio remoto de confianza.
- El trust store es local al `HLI_HOME`; no se sincroniza entre máquinas.
- Los permisos del archivo de clave privada se restringen con ACL de Windows
  (`icacls`) o `0600` en Unix; si el sistema no permite aplicarlos, el archivo
  sigue dentro de `HLI_HOME` y nunca se imprime.
- La rotación es manual: generar clave nueva, firmar de nuevo y confiar en la
  nueva clave. La revocación es local y no invalida copias externas.
