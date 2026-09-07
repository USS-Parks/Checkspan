# Checkspan

*Work you can verify.*

Checkspan is a new, independent product exploration: a workflow engine for multi-agent RAG and verified software work, with explicit acceptance criteria, traceable evidence, and human decisions.

The name combines a **check** at each acceptance boundary with the **span** of connected work across dependencies.

The planned first complete pilot is request + approved corpus + supplied patch → multi-agent cited research + software checks → review packet. Requirements and implementation retrieval agents feed a synthesizer and challenger; a designated checker enforces the evidence contract. Agent agreement cannot create acceptance.

**Repository:** [USS-Parks/Checkspan](https://github.com/USS-Parks/Checkspan)

## What works today

`checkspan validate` and `checkspan inspect` check any record document offline: bounded parsing, bundled JSON Schemas, record contracts, and graph admission, with no network access and no writes. Start with the [validation guide](docs/CHECKSPAN-M1-GUIDE.md) and the [examples](examples/README.md).

`checkspan run` operates a local patch → check workflow against a durable SQLite ledger: it captures the exact candidate from a Git repository, freezes scoped evidence, runs the pinned software checks as bounded child processes, admits verifier receipts, applies retry budgets, and opens a human gate packet when work exhausts its attempts. Progress is durable; a fresh process resumes where the ledger says the run is. The [ledger guide](docs/CHECKSPAN-M2-GUIDE.md) explains the storage and recovery model.

```bash
cargo build --release --locked && target/release/checkspan inspect examples/graphs/review-pilot.json
```

Not yet built: externally signed operator decisions, review packet export, corpus admission and retrieval, model-backed research agents, GitHub-backed evidence, and packaged binaries. An unimplemented capability is refused with a named reason, never faked.

The core and CLI are Rust (toolchain pinned in `rust-toolchain.toml`); schemas are JSON Schema Draft 2020-12 bundled into the binary.

## Project records

- [Plan](PLANNING/CHECKSPAN-PSPR.md) — the approved scope and its gates
- [Development log](docs/CHECKSPAN-DEVLOG.md) — execution state, prompt by prompt
- [Verification ledger](docs/CHECKSPAN-VERIFICATION.md) — what has been proven, where, with which evidence
- [Dependency and toolchain record](docs/CHECKSPAN-DEPENDENCIES.md)

## Current papers

- [Product one-pager — v0.3](CHECKSPAN-ONE-PAGER.md)
- [Schema, state transitions, and 27 paper review cases — v0.3](CHECKSPAN-SCHEMA-NOTES.md)

These are the current exploration documents; earlier scope stays visible in Git.

## Historical drafts

- [Original one-pager — v0.1](WORK-GRAPH-ONE-PAGER.md)
- [Original schema review — v0.1](WORK-GRAPH-SCHEMA-NOTES.md)

The historical files remain unchanged for traceability. Existing exclusions and project locks remain in force.
