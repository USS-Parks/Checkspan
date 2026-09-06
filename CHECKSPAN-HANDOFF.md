# Checkspan — session handoff

**Written:** 2026-09-06, end of the first implementation session.
**Repository:** [USS-Parks/Checkspan](https://github.com/USS-Parks/Checkspan). **Canonical checkout:** `C:\Users\17076\Documents\Codex\Work Graph Project`, branch `codex/checkspan-m1`, which equals `main`.
**Plan:** [PLANNING/CHECKSPAN-PSPR.md](PLANNING/CHECKSPAN-PSPR.md) (Draft 0.2). **Log:** [docs/CHECKSPAN-DEVLOG.md](docs/CHECKSPAN-DEVLOG.md). **Ledger:** [docs/CHECKSPAN-VERIFICATION.md](docs/CHECKSPAN-VERIFICATION.md).

## Where things stand

- **M1 (CS-01–CS-07) accepted** at `ac639a4` with native Windows, native Linux, and hosted evidence (`test-evidence/checkspan/CS-07/`).
- **M2 (CS-08–CS-14) accepted** at `cb63a14daf5f48476de58dc856283d67e96d9ab1`: 142 tests pass on Windows and on Linux, the recovery exercise passes with real killed and competing processes, the inspection output is byte-identical across platforms (`test-evidence/checkspan/CS-14/`), and the hosted matrix passed on both refs (runs 34064484446 and 34064487105).
- **Execution is stopped at the M2 milestone boundary.** CS-15 (M3) needs Basho's explicit approval ("Run M3 STS"). The full-STS approval does not erase milestone stops (PSPR §0.3).

## First tasks for the next session

1. Read PSPR §0 and the CANON, then the DEVLOG's execution-state block, before doing anything.
2. Wait for Basho's M3 approval before touching CS-15. M3 is the first complete pilot (CS-15–CS-24, then CS-R01–CS-R12, then CS-25) and needs, before its live prompts: an admitted non-sensitive corpus, an approved model runtime and usage budget (CS-R05), a Basho-controlled signing identity (CS-22), and a default store location outside any candidate checkout (CS-15/CS-20).

## Working routine (Basho's standing instructions)

- One focused commit per roster prompt on `codex/checkspan-m1`, then `git merge --ff-only` into `main` and push both refs. Basho said this explicitly on 2026-09-06.
- Every commit ends with `Authored and reviewed by Basho Parks, Copyright 2026` and carries no AI co-author trailer (CANON §3). The repo's `pre-commit` and `pre-push` hooks run the no-slop scan.
- Before each commit: `cargo fmt --all -- --check`, `cargo clippy --locked --all-targets -- -D warnings`, `cargo test --locked`; `cargo deny check` and `cargo audit` whenever dependencies change (record the review in `docs/CHECKSPAN-DEPENDENCIES.md`).
- Each prompt gets a DEVLOG entry (source SHA before work, changed paths, commands and outcomes, gate evidence, not-claimed list, storage closeout at milestones) and a ledger update; the implementation commit's SHA is recorded in the *next* prompt's entry.
- Native Linux evidence: WSL2 Ubuntu has rustup 1.29.1 and toolchain 1.98.0; the clone at `/home/basho/checkspan-m1` is detached at cb63a14. Fetch and check out the SHA under test there (run `git clean -fdq -- test-evidence` first if untracked evidence files block the checkout). Build in the WSL filesystem, not through `/mnt/c`.
- Milestone boundaries and the other explicit stops in PSPR §0.3 are hard stops: report evidence, blockers, and the next milestone, then end the turn.

## Code map (all in one package, `checkspan`)

| Module | Prompt | What it owns |
| --- | --- | --- |
| `contracts/` | CS-02–CS-04, CS-11 | Records (`GraphSpec`, `GraphRun` with `admitted_imports`, `NodeSpec`, `EvidenceRef`, `Attempt`, `VerifierReceipt`, `GatePacket`, `GateDecision`, `NodeView`), identity primitives, header-first parsing, bundled schemas (`schemas/v1/`, `$id` base `https://checkspan.invalid/schemas/v1/`) |
| `validation/` | CS-05 | Staged bounded validation: size, syntax, depth, duplicate keys, header, schema (frozen keyword set, offline `$ref`), shape, contract, admission |
| `graph/` | CS-06 | Admission: dependency resolution, port and type checks, cycles, proof ports, gate wait cycles, deterministic order, required closure |
| `cli` | CS-01, CS-06 | `checkspan validate` / `inspect`; exit 0/1/2/3; `examples/` |
| `digests/` | CS-08 | SHA-256 artifact digests; RFC 8785 envelopes with an integer-only policy; typed digests for graph, node, input manifest, verification context |
| `store/` | CS-09, CS-12 | SQLite ledger (schema 2: records, event log, claims), `Ledger` read trait on `Store` and `Tx`, composite atomic operations, revocation events, migrations |
| `state/` | CS-10 | Pure reducer over typed events; `RunState::replay` into views |
| `deps/` | CS-11 | Run-scoped dependency resolution, explicit imports, freshness and revocation, dispatch snapshot and pre-acceptance recheck |
| `scheduler/` | CS-12 | Fenced claims in one `BEGIN IMMEDIATE` transaction, active-claim cap, resource conflicts, complete/cancel/reclaim, views with blocked reasons |
| `budget/` | CS-13 | Per-node caps, graph budget with lineage and deadline, failure-class routing, narrowing rules |

Tests live in `tests/*.rs` with shared helpers in `tests/common/mod.rs` and fixtures under `tests/fixtures/`. The JSON fixtures are authoritative; the Python generators used to produce them were session-local scratch and are gone, so edit fixtures directly or write new tooling if large regeneration is needed.

## Decisions Basho may want to revisit

- Schema IDs use the reserved host `checkspan.invalid` (never resolves; bundled registry only).
- `jsonschema` is a runtime dependency with default features off; `serde_json_canonicalizer` and `sha2` were chosen for CS-08; `rusqlite` with bundled SQLite for CS-09; `deny.toml` allows MIT, Apache-2.0, Unicode-3.0, MIT-0, Zlib.
- Envelope digests refuse floats and integers beyond ±2^53.
- The single-worker cap is a scheduler setting (default 1) rather than a database constraint, so CS-R04 can raise it; resource conflicts are enforced regardless.
- A human gate decision may reopen a node past its automatic attempt cap, never past the graph budget.
- Exhaustion routed to `gate` is reported, not acted on, until CS-21 builds packets.

## Retained storage (removal needs Basho's authorization)

`target/` (about 2.6 GB, ignored) in the canonical checkout; the WSL clone and its `target/`; the WSL rustup toolchain. No extra git worktree exists. The `checkspan-*` test scratch directories under the Windows Temp folder and WSL `/tmp` were removed on Basho's instruction on 2026-09-06; tests recreate them as needed.

## Memory notes for Claude sessions

Two memory files in the Claude project memory directory cover the routine and the evidence locations: `checkspan-roster-workflow` and `checkspan-milestone-evidence-locations`.
