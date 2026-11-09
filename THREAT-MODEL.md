# HLI — Modelo de amenazas

| Activo | Amenaza | Mitigación | Límite |
|---|---|---|---|
| Event log | alteración silenciosa | cadena SHA-256 + verificación | no hay firma del log |
| Paquetes `.hlimod` | sustitución de binario | checksums + firma Ed25519 + trust store | sin PKI/revocación global |
| Secretos | fuga a logs/export | solo `secrets/` o env; no se serializan | usuario con acceso al FS puede leerlos |
| Base de datos | corrupción/parcial | transacciones, migraciones, backup | restore manual |
| MCH | bucle de procesos | límite de reinicios y backoff | no es sandbox |
| MCP | cliente malicioso local | loopback + stdio del cliente | sin autenticación remota |
| Plan/aprobación | aprobar algo distinto de lo revisado | hash del plan + hash de la instantánea + `plan_id` obligatorio | el usuario debe revisar el plan mostrado |
| Instantánea de ejecución | filtración de secretos | sólo `id@versión`; nunca valores; MCP sólo expone el hash | acceso al FS de `HLI_HOME` sigue siendo del usuario |
| Instalador | PATH/dirs peligrosos | rutas prohibidas y rollback | no valida firma del paquete |

Fuera de alcance: atacante con privilegios de administrador, malware local,
canales encubiertos de red, aislamiento de CPU/memoria.
