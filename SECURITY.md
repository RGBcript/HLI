# Seguridad

## Reportar

No abras un issue público con detalles explotables. Contacta al mantenedor del
repositorio de forma privada y describe versión, impacto y reproducción.

## Modelo

- HLI es local: daemon en loopback, datos en `HLI_HOME`.
- Secretos en `HLI_HOME/secrets/` o `HLI_SECRET_*`; nunca en base, eventos,
  exportaciones ni logs.
- La instantánea de ejecución (`ExecSnapshot`) guarda sólo identificadores y
  versiones de los handles (`id@v…`), jamás valores. El plan público no incluye
  la instantánea; MCP expone únicamente su hash.
- Al aprobar se verifica el hash de la instantánea y su vigencia: si un secreto
  cambió de versión, la aprobación se invalida y se exige un replan.
- Módulos son procesos locales; la firma Ed25519 y el trust store permiten
  política `require-signed`, pero no son un sandbox.
- Los backups de base no incluyen secretos.
- `shell.execute` está denegado.

## Alcance

No se promete aislamiento del sistema operativo ni resistencia a un atacante con
control del usuario. Ver `THREAT-MODEL.md`.

---

## Permisos por invocación

- El plan aprobado autoriza efectos; cada invocación concreta se resuelve con la
  política (`deny` → `ask` → grants `once|run|workspace`).
- `arguments_hash` congela la invocación: una solicitud antigua no autoriza
  argumentos nuevos.
- `shell.execute` se deniega siempre en HLI 1.0.
- La invocación privada nunca se expone por CLI, MCP ni TUI; el receipt guarda
  sólo hash, resumen y decisión.
- Los valores de secreto no se persisten en la solicitud: si aparecen claves
  sensibles en los argumentos, se redactan y la solicitud no se reanuda.

### Herramientas no declaradas

Una invocación cuya herramienta o efecto no figura en el plan aprobado no se
ejecuta bajo ninguna circunstancia. HLI crea una revisión del plan, la somete a
una aprobación nueva y sólo después resuelve el permiso concreto. Esto impide
que una aprobación antigua autorice una operación que el usuario nunca revisó.
