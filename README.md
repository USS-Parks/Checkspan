# Checkspan

*Work you can verify.*

Checkspan is a new, independent product exploration: a workflow engine for multi-agent RAG and verified software work, with explicit acceptance criteria, traceable evidence, and human decisions.

The name combines a **check** at each acceptance boundary with the **span** of connected work across dependencies. Proposed CLI and repository name: `checkspan`.

The proposed first complete pilot is request + approved corpus + supplied patch → multi-agent cited research + software checks → review packet. Requirements and implementation retrieval agents feed a synthesizer and challenger; a designated checker enforces the evidence contract. Agent agreement cannot create acceptance.

**Repository:** [USS-Parks/Checkspan](https://github.com/USS-Parks/Checkspan)

## Review and approval

The [canonical PSPR](PLANNING/CHECKSPAN-PSPR.md) is **Draft 0.2 for review: 48 ordered prompts across five independently approvable milestones**. All implementation prompts are unstarted and unapproved. The proposed first approval is **M1: the offline contract explorer**.

- [Development log](docs/CHECKSPAN-DEVLOG.md)
- [Verification ledger](docs/CHECKSPAN-VERIFICATION.md)

The original CS-01–CS-36 IDs remain stable. CS-R01–CS-R12 add corpus admission, real model-backed roles, bounded research concurrency, grounding checks, and evaluation before M3 acceptance at CS-25.

Rust is the proposed core/CLI language. The PSPR records the complete stack recommendation and override points; it is not an already-approved implementation choice.

## Current papers

- [Product one-pager — v0.3](CHECKSPAN-ONE-PAGER.md)
- [Schema, state transitions, and 27 paper review cases — v0.3](CHECKSPAN-SCHEMA-NOTES.md)

These are the current exploration documents. The current revision adds the missing multi-agent RAG workflow while preserving the independent product identity. Previous scope remains visible in Git and the revision histories. No implementation is authorized.

## Historical drafts

- [Original one-pager — v0.1](WORK-GRAPH-ONE-PAGER.md)
- [Original schema review — v0.1](WORK-GRAPH-SCHEMA-NOTES.md)

The historical files remain unchanged for traceability. Existing exclusions and project locks remain in force.
