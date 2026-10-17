# Codex integration without a skill command

xnip is an editing tool. For automatic use without a `$xnip-editing` entry, install the [Codex instruction package](../integrations/codex/AGENTS.md) as ordinary agent instructions and a guide. This integration is available in source after v0.2.0; its archive will ship with the next release.

## Install

1. Install the xnip executable using the [normal installation instructions](../README.md#install). Optionally configure its [MCP server](mcp.md#client-configuration) for structured editing calls.
2. Create `~/.codex/guides/xnip-editing/`. Copy `guide.md` and the complete `references/` directory from `integrations/codex/` into it. Keep these files outside skill discovery directories.
3. Merge the short `integrations/codex/AGENTS.md` section into `~/.codex/AGENTS.md`. Preserve existing instructions. If `CODEX_HOME` is customized, use that home and update the guide path in the section.
4. If migrating from the old installation, move `~/.codex/skills/xnip-editing/` outside the skills directory, preserving any local customizations. Check `~/.agents/skills/` and repository skill directories for additional installed copies.
5. Start a new Codex task/session so it loads the instructions and refreshes skill discovery. An existing conversation may retain an already-attached skill until its context is refreshed.

Codex now receives a small rule for text-editing tasks and reads the detailed guide when needed. It calls `xnip_edit`, `xnip_apply` and other MCP tools, or the CLI, directly. The guide is a plain document with no skill frontmatter or registered skill name, so it provides no `$xnip-editing` selector entry. The registered MCP tools remain available for the agent to perform edits. Users can still request particular tool behavior in ordinary language; this integration controls the skill entry, not user authority.

## Why use instructions instead of an automatic-only skill?

Codex documents explicit and implicit skill invocation. Its `allow_implicit_invocation` setting controls automatic selection; setting it to false disables that selection. Codex currently exposes no corresponding option to retain automatic skill selection while hiding only its manual entry. See the [official skill documentation](https://learn.chatgpt.com/docs/build-skills#optional-metadata). Adding another client's `user-invocable: false` field does not implement that behavior in Codex.

The repository still offers [generic/GitHub](../.github/skills/xnip-editing/SKILL.md) and [Claude Code](../integrations/claude-code/SKILL.md) skills for clients and users who want skill discovery and manual entries. Those are optional integrations; use the Codex instructions above when no manual skill command is wanted.

## Updating

Refresh `guide.md` and `references/` from the current instruction package. Review local changes before replacing files. The guide is generated from `docs/SKILL.md` by `cargo run --locked -p xtask -- sync-integrations`; it shares the editing rules with the skill packages without registering a skill. Merge changes to the short AGENTS section rather than overwriting a global AGENTS file.
