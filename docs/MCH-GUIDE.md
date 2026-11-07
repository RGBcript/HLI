# HLI — Guía MCH

MCH (Managed Child Host) es una instancia especializada administrada por HLI.

```powershell
hli mch create memory-main --module hli-memory --workspace ws_...
hli mch serve memory-main
hli mch status memory-main
hli mch invoke memory-main memory.search --args '{"query":"límite"}'
hli mch stop memory-main
```

- Lifecycle completo antes de `ready`; `invoke` exige `ready`.
- Health periódico: degraded a los 2 fallos, reinicio al tercero.
- Máximo 3 reinicios automáticos por ventana de 60 s; después `failed`.
- `mch restart` recupera manualmente y limpia el limitador.
- `--autostart` relanza la instancia al arrancar el daemon.
- PID y estado se normalizan al abrir el store; no se reutilizan PIDs antiguos.

Detalle técnico y límites: `docs/MCH-SUPERVISOR.md`.
