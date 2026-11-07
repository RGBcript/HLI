# Seguridad

## Reportar

No abras un issue público con detalles explotables. Contacta al mantenedor del
repositorio de forma privada y describe versión, impacto y reproducción.

## Modelo

- HLI es local: daemon en loopback, datos en `HLI_HOME`.
- Secretos en `HLI_HOME/secrets/` o `HLI_SECRET_*`; nunca en base, eventos,
  exportaciones ni logs.
- Módulos son procesos locales; la firma Ed25519 y el trust store permiten
  política `require-signed`, pero no son un sandbox.
- Los backups de base no incluyen secretos.
- `shell.execute` está denegado.

## Alcance

No se promete aislamiento del sistema operativo ni resistencia a un atacante con
control del usuario. Ver `THREAT-MODEL.md`.
