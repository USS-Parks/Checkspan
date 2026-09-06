# Checkspan

*Work you can verify.*

Checkspan is a new, independent product exploration: a workflow engine for AI-assisted work with explicit acceptance criteria, traceable evidence, and human decisions.

The name combines a **check** at each acceptance boundary with the **span** of connected work across dependencies. Proposed CLI and repository name: `checkspan`.

The first product workflow is a typed patch artifact → CI check → review packet. Every acceptance is bound to its stated claim, exact candidate, and supporting evidence.

**Repository:** [USS-Parks/Checkspan](https://github.com/USS-Parks/Checkspan)

## Review and approval

The [canonical PSPR](PLANNING/CHECKSPAN-PSPR.md) is **drafted for review: 36 ordered prompts across five independently approvable milestones**. All implementation prompts are unstarted and unapproved. The proposed first approval is **M1: the offline contract explorer**.

- [Development log](docs/CHECKSPAN-DEVLOG.md)
- [Verification ledger](docs/CHECKSPAN-VERIFICATION.md)

Rust is the proposed core/CLI language. The PSPR records the complete stack recommendation and override points; it is not an already-approved implementation choice.

## Current papers

- [Product one-pager — v0.2](CHECKSPAN-ONE-PAGER.md)
- [Schema, state transitions, and 15 paper review cases — v0.2](CHECKSPAN-SCHEMA-NOTES.md)

These are the current exploration documents. This naming revision establishes an independent product identity and supersedes the earlier positioning around other products. No implementation is authorized.

## Historical drafts

- [Original one-pager — v0.1](WORK-GRAPH-ONE-PAGER.md)
- [Original schema review — v0.1](WORK-GRAPH-SCHEMA-NOTES.md)

The historical files remain unchanged for traceability. Existing exclusions and project locks remain in force.
