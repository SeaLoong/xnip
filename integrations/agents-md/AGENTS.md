# xnip editing

Use the configured xnip MCP tools or CLI for supported text edits; respect the user's editor choice. Reuse known context and read only missing context.

For a known block, use `xnip_edit({file,old,text})` or `xnip edit FILE --old OLD --text NEW`. Match a unique nonempty block; extend ambiguous context. Add `position:"before"` or `"after"` for exact insertion without added newlines; use `all` only for global replacement. Whole-file create-or-replace uses `operation:"write",text:CONTENT` (`--write`), with optional `old` guarding the entire existing file. Create/delete/rename use `edit`. Independent files use `xnip_apply({files:{"a":[["old","new"]],"b":"complete content"}})`; ordered lifecycle and patches use manifests.

`dry_run` previews and `check` validates without writing. Read the receipt; inspect `mode` and `written` when structured results are enabled. On failure, inspect the error and relevant context; inspect any recovery journal before retrying.

Read the [agent guide](https://github.com/SeaLoong/xnip/blob/master/docs/SKILL.md) for operation selection and recovery, then load only the needed reference. Search and language tools handle discovery and symbol analysis.
