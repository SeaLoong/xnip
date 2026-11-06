# Historical benchmark evidence

These records preserve the inputs, implementations, schemas and availability
captured by each run. The [current benchmark index](../README.md) and
[support matrix](../../support.md) describe today's integration routes.
Original result paths and raw fingerprints stay available for reproduction.

| Recorded study | Evidence |
|---|---|
| 0.6.0 candidate, 1,878 measured workflows; dependency gate failed before publication | [Snapshot and reports](2026-11-06-v0.6.0-candidate/README.md) · [Original hashes](2026-11-06-v0.6.0-candidate/index.json) |
| Ten integration cohorts before 0.6.0, 1,878 measured workflows | [Snapshot and reports](2026-11-06-pre-0.6.0/README.md) · [Original hashes](2026-11-06-pre-0.6.0/index.json) |
| 13 editing tasks / 41 editor profiles, xnip 0.5.5 | [Report](../results.md) · [Raw run](../results.json) · [Derived frontier](../relative-results.json) |
| Six host-to-xnip MCP bridge profiles | [Captured exchanges](../host-exchange-results.json) |
| Supplementary representation, catalog and receipt measurements | [Report](../representation-results.md) · [Raw run](../representation-results.json) · [Method](../representation-methodology.md) |
| Core/CLI performance snapshot, xnip 0.4.0 | [Report](../core-performance.md) · [Criterion samples](../core-criterion-results.json) · [CLI samples](../core-performance-results.json) |
| Initial DSH/Pi provider studies | [DSH](../native-provider-results.md) · [Pi](../pi-provider-results.md) |
| Fixed-binary DSH encoding comparison | [Control](../native-provider-encoding.md) · [Raw samples](../native-provider-encoding.json) |

Historical native-editor availability describes the recorded executor cohort.
Current client profiles, plugins and SDK toolsets have their own contracts and
measurements. The original 13-task frontier retains its original profiles and
token accounting; current integration cohorts retain their named API and
filesystem boundaries.
