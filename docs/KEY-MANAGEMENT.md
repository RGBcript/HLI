# HLI — Gestión de claves y confianza

**Estado:** implementado junto con la firma de módulos.

## Layout

```
HLI_HOME/
  keys/
    index.json          # metadatos públicos: key_id, name, public_key, fingerprint, revoked_at
    key_<hex>.key       # clave privada (base64 del seed), permisos restringidos
  trust.json            # trust store local
  module-policy.json    # política de aceptación (opcional)
```

- La clave privada **no** se guarda en SQLite, eventos, manifests, receipts ni
  logs. Solo existe en `keys/<key_id>.key`.
- El índice de claves y el trust store solo contienen material público.
- `key_id = "key_" + primeros 16 hex de sha256(public_key)`.
- `fingerprint = "sha256:" + sha256(public_key)`.

## Permisos

- Unix: `0600` sobre el archivo de clave.
- Windows: `icacls <archivo> /inheritance:r /grant:r <usuario>:F` (best effort).
- Si el sistema no aplica la ACL, el archivo permanece dentro de `HLI_HOME`; no
  se imprime ni se serializa en ninguna salida.

## Ciclo de vida

```powershell
hli key generate release-2026
hli key list
hli key inspect key_...
hli key export-public key_...     # JSON con public_key y fingerprint
hli key revoke key_...            # deja de firmar; las firmas existentes pasan a signed-revoked
```

- `key generate` crea par Ed25519 y escribe la clave privada restringida.
- `key export-public` es la vía para compartir la clave pública; nunca exporta la
  privada.
- `key revoke` marca la clave en el índice local. No borra archivos ni firmas.

## Trust store

```powershell
hli trust add <public-key-file>
hli trust list
hli trust remove <key_id>
hli trust revoke <key_id>
```

Campos por entrada:

| Campo | Descripción |
|---|---|
| `key_id` | identificador derivado de la clave |
| `public_key` | clave pública en base64 |
| `fingerprint` | `sha256:` de la clave pública |
| `label` | etiqueta legible |
| `trusted_at` | fecha de confianza explícita |
| `revoked_at` | fecha de revocación local (null si vigente) |

`trust add` acepta el JSON de `key export-public` o un archivo con la clave en
base64. Valida que el fingerprint y el `key_id` declarados correspondan a la
clave; si no, rechaza la importación.

Una firma criptográficamente válida **no** se confía automáticamente: sin
entrada en el trust store el estado es `signed-untrusted` y `require-signed` la
rechaza.

## Rotación y revocación

1. Generar clave nueva con `key generate`.
2. Exportar la pública y añadirla con `trust add`.
3. Firmar de nuevo los paquetes con `module sign --key <nueva>`.
4. Revocar la anterior con `trust revoke` (y `key revoke` si era local).

Revocar una clave no desinstala módulos: los ya instalados conservan su estado y
proceso, pero `module list` los marca `signed-revoked` y las nuevas
instalaciones se rechazan.

## Buenas prácticas

- No copiar `keys/` a repositorios, artefactos ni backups compartidos.
- No ejecutar `key export-public` esperando ver la privada: nunca se imprime.
- Mantener `module-policy.json` en `require-signed` para instalaciones públicas;
  `warn-unsigned` es el default de desarrollo local.
- Revisar `trust list` antes de instalar paquetes de terceros.

## Limitaciones

- Sin HSM, PKI ni X.509.
- Sin sincronización remota del trust store.
- Sin sellado de tiempo externo: `signed_at` es la hora local del firmante.
- La revocación es local; una copia del paquete en otra máquina con otra
  confianza puede seguir aceptándose según su política.
