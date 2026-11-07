# HLI — Informe de prueba de instalación

**Paquete:** `hli-0.9.0-beta.1-windows-x86_64`
**Gate:** `scripts/install_gate.ps1`

Secuencia ejecutada sobre directorios temporales independientes (bin, datos y
`HLI_HOME`), sin tocar el perfil real:

1. Verificar `checksums.sha256`.
2. Instalar con `install.ps1 -NoPath`.
3. `hli --version` → `hli 0.9.0-beta.1`.
4. Arrancar daemon.
5. Crear workspace, thread y branch.
6. Configurar SimProvider y ejecutar un run completado.
7. Instalar `hli-memory-0.9.0-beta.1.hlimod` firmado (`signed-trusted`).
8. Crear MCH Memory, servirlo e invocar `memory.search`.
9. Registrar y recuperar memoria.
10. Reiniciar daemon y comprobar persistencia.
11. Exportar workspace.
12. Desinstalar y confirmar que `HLI_HOME`/`hli.db` siguen.
13. Reinstalar.
14. Importar el workspace en un `HLI_HOME` limpio y verificar estado equivalente.

Resultado:

```json
{
  "gate": "windows-install",
  "ok": true,
  "checksums": true,
  "install": true,
  "version": true,
  "daemon": true,
  "run": true,
  "mch_memory": true,
  "restart": true,
  "export_import": true,
  "uninstall_preserved_data": true,
  "reinstall": true
}
```

Observaciones: el instalador no modifica el PATH con `-NoPath`; el
desinstalador conserva datos por defecto y solo borra con `-RemoveData` más
confirmación (o `-ForceData` en modo no interactivo).
