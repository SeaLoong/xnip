# Integration snapshot before 0.6.0

This snapshot preserves ten integration cohorts and **1,878 measured workflows**
from the release executable with SHA-256
`16cecdbad176790492338643e29480d2dc7793c5cda5629102e0dde434be8deb`.
Each report retains its recorded SDK versions, source fingerprints, timing
boundaries, host-policy outcomes and literal-byte goals.

The raw JSON files preserve their original bytes. [index.json](index.json)
records each raw file's SHA-256 and measured-workflow count. Markdown companions
preserve the report text and tables; only relative links are rebased for this
directory. Their original and archived SHA-256 values appear as
`markdown_original_sha256` and `markdown_sha256` in the same index.

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
its own source and executable identity. The [current benchmark index](../../README.md)
and [historical index](../README.md) identify the other recorded snapshots.
