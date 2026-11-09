# HLI — Referencia CLI

Todas las salidas aceptan `--output json|human`.

```text
hli --version
hli daemon start|stop|status
hli workspace create <name> [--root PATH] | list | get <id>
hli thread create <title> --workspace <id> | list --workspace <id>
hli branch create <name> --thread <id> [--from EVENT] | list --thread <id> | show <id> | snapshot <id>
hli event append --type T --payload JSON --workspace <id> [--thread ID] [--branch ID] [--idem K]
hli event list --workspace <id> [--branch ID]
hli artifact put <file> --workspace <id> | list --workspace <id>
hli artifact slice <artifact_id> --workspace <id> [--kind K] [--start N] [--end M] [--reason R]
hli run start --workspace <id> --thread <id> --branch <id> --prompt PROMPT [--provider P] [--model M] [--base-url U] [--secret HANDLE]
hli run get <run_id> | list --workspace <id> | status <run_id> | wait <run_id>
hli run plan <run_id>
hli run approve <run_id> [--scope once|run|workspace] [--plan PLAN_ID]
hli run reject <run_id> --reason TEXTO [--plan PLAN_ID]
hli run replan <run_id> [--plan PLAN_ID] [--reason TEXTO]...
hli permission list [--run ID] [--status pending]
hli permission inspect <request_id>
hli permission approve <request_id> --scope once|run|workspace
hli permission deny <request_id> [--reason TEXTO]
hli permission cancel <request_id>
hli manifest get <manifest_id>
hli prices list
hli module install <file.hlimod> | list | inspect <name> | test <name> | remove <name>
hli module sign <file.hlimod> --key <key_id> | verify <file.hlimod>
hli key generate <name> | list | inspect <key_id> | export-public <key_id> | revoke <key_id>
hli trust add <public-key-file> | list | remove <key_id> | revoke <key_id>
hli migrate status | backup | apply | verify
hli mch create <name> [--module M] [--domain D] [--workspace W] [--autostart] | list | inspect <name>
hli mch set-autostart <name> true|false
hli mch serve <name> | stop <name> | remove <name>
hli mch invoke <name> <operation> --args JSON [--workspace W]
hli memory commit --workspace <id> --type TYPE --content TEXT [--scope S]
hli memory search --workspace <id> --query TEXT | get <id> | list --workspace <id> | forget <id>
hli verify --workspace <id>
hli export <workspace_id> --out <file>
hli import <file>
hli status
```

`hli` sin argumentos abre la TUI si hay terminal. `hli --version` imprime la
versión del paquete.
