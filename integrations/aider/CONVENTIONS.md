# xnip editing conventions

Use the configured xnip MCP tools or CLI for supported text edits; respect the user's editor choice. Reuse known context and read only missing context.

For a known block, use `xnip_edit({file,old,text})` or `xnip edit FILE --old OLD --text NEW`. Matching is literal and unique; extend the old block when ambiguous. Use `all` only for an intentional global change. Create/delete/rename use `edit`; several blocks use `xnip_apply({edits:[{file,old,text},...]})`; manifests and patches use `apply`.

`dry_run` previews and `check` validates without writing. Inspect `mode` and `written` in structured results. On failure, inspect the error and relevant context; inspect any recovery journal before retrying.

Read the [agent guide](https://github.com/SeaLoong/xnip/blob/master/docs/SKILL.md) for operation selection and recovery, then load only the needed reference. Search and language tools handle discovery and symbol analysis.
