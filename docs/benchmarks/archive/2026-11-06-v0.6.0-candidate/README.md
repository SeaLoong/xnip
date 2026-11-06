# 0.6.0 candidate benchmark snapshot

These ten cohorts contain **1,878 measured workflows** from the 0.6.0 release
candidate. Its dependency gate failed on
[RUSTSEC-2026-0386](https://rustsec.org/advisories/RUSTSEC-2026-0386.html) in
Tokio 1.52.3, so the candidate was not published. The existing `v0.6.0` tag
remains unchanged.

The recorded release executable has SHA-256
`144f76db0a340d7eede61b7fc2545c8c7c1d7d937300f378fd6c77c8da3e0688`.
Reports retain their SDK versions, source fingerprints, timing boundaries,
host-policy outcomes and literal-byte goals.

Raw JSON files preserve their original bytes. [index.json](index.json) records
each raw SHA-256 and workflow count, plus `markdown_original_sha256` and
`markdown_sha256`. Markdown companions preserve all report text and tables;
only relative links are rebased for this archive.

| Cohort | Report | Raw data | Measured workflows |
|---|---|---|---:|
| OpenCode | [Markdown](opencode-provider-results.md) | [JSON](opencode-provider-results.json) | 138 |
| OpenHands | [Markdown](openhands-provider-results.md) | [JSON](openhands-provider-results.json) | 138 |
| aider | [Markdown](aider-provider-results.md) | [JSON](aider-provider-results.json) | 426 |
| DeepAgents / LangGraph / PydanticAI | [Markdown](python-agents-provider-results.md) | [JSON](python-agents-provider-results.json) | 276 |
| OpenAI Agents SDK | [Markdown](agents-sdk-provider-results.md) | [JSON](agents-sdk-provider-results.json) | 156 |
| AutoGen | [Markdown](autogen-provider-results.md) | [JSON](autogen-provider-results.json) | 156 |
| smolagents | [Markdown](smolagents-provider-results.md) | [JSON](smolagents-provider-results.json) | 156 |
| Google ADK | [Markdown](google-adk-provider-results.md) | [JSON](google-adk-provider-results.json) | 156 |
| CrewAI | [Markdown](crewai-provider-results.md) | [JSON](crewai-provider-results.json) | 156 |
| Client profiles / Goose | [Markdown](host-profiles-results.md) | [JSON](host-profiles-results.json) | 120 |

Reproduction commands run from the repository root and produce a new run with
its own source and executable identity. See the [current benchmark index](../../README.md)
and [historical index](../README.md) for the other recorded snapshots.
