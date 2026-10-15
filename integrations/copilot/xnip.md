# xnip editing workflow for Copilot

Use xnip as the normal text-file editing interface when it supports the intended operation. Respect explicit user tool choices. Reuse known context; read missing context only when needed, without a mandatory doctor/peek/find sequence.

For known text blocks, use `xnip edit FILE --old-file OLD --text-file NEW`: matching is literal and must be unique unless `--all` is intentional. `edit --create` creates missing files; `edit --delete` deletes regular files. `--dry-run`/`--check` are available in CLI and the `xnip_edit` MCP tool. These capabilities require the development build; v0.1.1 uses the older replace/insert commands.

Use line/anchor commands for those locations, and `apply` for disjoint edits to existing files. Search and semantic analysis remain separate tasks; use repository/language tools as appropriate. Whole-file rename and general patch import are not yet supported, so use a suitable tool rather than a risky delete/create workaround.

Do not install software or register MCP merely because these instructions are loaded. On an unavailable capability, use an existing editor and report the limitation. Batch commits are per-file; inspect committed paths after exit 4 instead of blindly retrying. Byte preconditions are not locks; Git/backups provide exact recovery.

Read the [agent guide](https://github.com/SeaLoong/xnip/blob/master/docs/SKILL.md) and relevant references only as needed.
