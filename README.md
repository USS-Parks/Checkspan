# Checkspan

*Work you can verify.*

Checkspan is a new, independent product exploration: a workflow engine for multi-agent RAG and verified software work, with explicit acceptance criteria, traceable evidence, and human decisions.

The name combines a **check** at each acceptance boundary with the **span** of connected work across dependencies. Proposed CLI and repository name: `checkspan`.

The proposed first complete pilot is request + approved corpus + supplied patch → multi-agent cited research + software checks → review packet. Requirements and implementation retrieval agents feed a synthesizer and challenger; a designated checker enforces the evidence contract. Agent agreement cannot create acceptance.

**Repository:** [USS-Parks/Checkspan](https://github.com/USS-Parks/Checkspan)

## Status

The [canonical PSPR](PLANNING/CHECKSPAN-PSPR.md) (Draft 0.2, 48 ordered prompts across five milestones) was approved for full STS on 2026-09-06. **M1, the offline contract explorer (CS-01–CS-07), is implemented**; execution stops at the M1 boundary pending Basho's M2 approval. Later milestones are unstarted.

M1 gives you `checkspan validate` and `checkspan inspect`: offline validation of every record kind and admission of graphs, with no worker, run store, credentials, or network. Start with the [M1 guide](docs/CHECKSPAN-M1-GUIDE.md) and the [examples](examples/README.md). M2 (CS-08–CS-14) adds the durable run ledger as library code: content digests, the SQLite store, the state reducer, run-scoped dependencies and imports, fenced claims, and persistent budgets; the [M2 guide](docs/CHECKSPAN-M2-GUIDE.md) explains recovery. M3 is in progress: CS-15 adds the `patch_result` record and a read-only Git adapter that captures the exact candidate a check will run against.

```bash
cargo build --release --locked && target/release/checkspan inspect examples/graphs/review-pilot.json
```

- [Development log](docs/CHECKSPAN-DEVLOG.md)
- [Verification ledger](docs/CHECKSPAN-VERIFICATION.md)
- [Dependency and toolchain record](docs/CHECKSPAN-DEPENDENCIES.md)

The original CS-01–CS-36 IDs remain stable. CS-R01–CS-R12 add corpus admission, real model-backed roles, bounded research concurrency, grounding checks, and evaluation before M3 acceptance at CS-25.

The core and CLI are Rust (toolchain pinned in `rust-toolchain.toml`); schemas are JSON Schema Draft 2020-12 bundled into the binary.

## Current papers

- [Product one-pager — v0.3](CHECKSPAN-ONE-PAGER.md)
- [Schema, state transitions, and 27 paper review cases — v0.3](CHECKSPAN-SCHEMA-NOTES.md)

These are the current exploration documents. The current revision adds the missing multi-agent RAG workflow while preserving the independent product identity. Previous scope remains visible in Git and the revision histories. No implementation is authorized.

## Historical drafts

- [Original one-pager — v0.1](WORK-GRAPH-ONE-PAGER.md)
- [Original schema review — v0.1](WORK-GRAPH-SCHEMA-NOTES.md)

The historical files remain unchanged for traceability. Existing exclusions and project locks remain in force.
