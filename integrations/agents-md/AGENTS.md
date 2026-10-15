# xnip editing workflow

Project instruction snippet for teams using xnip as their default text editor.

Use xnip as the normal text-file editing interface when it supports the intended operation. Respect explicit user tool choices. Reuse known context; read missing context only when needed, without a mandatory doctor/peek/find sequence.

For known text blocks, use `xnip edit FILE --old-file OLD --text-file NEW`: matching is literal and must be unique unless `--all` is intentional. `edit --create` creates missing files; `edit --delete` deletes regular files. `--dry-run`/`--check` are available in CLI and `xnip_edit`/`xnip_apply` MCP tools. These capabilities require v0.2.0; v0.1.1 uses the older replace/insert commands.

Use line/anchor commands for those locations, and `apply` for disjoint edits to existing files. Search and semantic analysis remain separate tasks; use repository/language tools as appropriate. Use `edit --rename-to` for whole-file moves and `apply --format patch` for text patch import. File lifecycle batches run in input order; `--sequential` enables dependent legacy edits.

Do not install software or register MCP merely because these instructions are loaded. On an unavailable capability, use an existing editor and report the limitation. Batch commits keep recovery snapshots and attempt rollback on failure; inspect rollback status and the journal after exit 4 instead of blindly retrying. Byte preconditions are not locks; Git/backups provide exact recovery.

Read the [agent guide](https://github.com/SeaLoong/xnip/blob/master/docs/SKILL.md) and relevant references only as needed.
