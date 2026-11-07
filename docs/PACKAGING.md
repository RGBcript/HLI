# HLI — Empaquetado Windows

```powershell
.\scripts\package_windows.ps1
```

Produce `dist/hli-0.9.0-beta.1-windows-x86_64/` con:

```
hli.exe  hli-daemon.exe  hli-mcp.exe  hli-module-memory.exe
hli-memory-0.9.0-beta.1.hlimod
install.ps1  uninstall.ps1
README.md  QUICKSTART.md  LICENSE  NOTICE  THIRD-PARTY-NOTICES.md
release-public.json  checksums.sha256  docs/
```

- Compila `cargo build --release --workspace`.
- Construye el `.hlimod` real y lo firma con una clave de release temporal; la
  privada se elimina y **nunca** se copia a `dist/`.
- Genera y verifica `checksums.sha256`.
- No incluye fixtures, secretos, bases, logs ni `target/`.
- El instalador vive en el propio paquete y no requiere administrador.

Regeneración reproducible: el script falla si un checksum o la versión no
coinciden.
