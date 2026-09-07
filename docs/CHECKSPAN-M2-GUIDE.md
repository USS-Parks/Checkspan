# Checkspan M2 — durable run ledger and recovery guide

M2 adds the local ledger under the M1 contracts: one SQLite file that records graphs, runs, attempts, receipts, gate packets, decisions, claims, and the ordered event log they hang off, plus the library that turns that log into node views, resolves dependencies, claims work, and enforces retry limits and budgets. Nothing in M2 executes a verifier, a worker command, or an external action; the run workflow commands arrive with M3. This guide explains how the ledger behaves when things go wrong and how to reproduce the acceptance evidence.

## What the ledger guarantees

- **One file is the truth.** Views are replayed from the event log; nothing cached can disagree with it. Reopening the file after any exit reproduces the same views, counters, and claims (see `tests/durable_recovery.rs`).
- **Every write is a transaction.** A record and the event that gives it meaning are written together or not at all. A controller that dies in the middle of a claim leaves no claim row and no dispatch event; SQLite's write-ahead log rolls the transaction back on the next open.
- **Acceptance needs its receipt.** A `receipt_admitted` event cannot exist without the receipt row (a trigger refuses it), and the reducer reaches `accepted` from nothing else. A restart cannot fabricate an accepted node.
- **Records are append-only.** Graphs, attempts, receipts, and events cannot be updated or deleted; a released claim cannot be changed again.
- **Completions are fenced.** Every claim carries a token. A completion is sealed only while the claim is active under that exact token and owner. After a cancellation or a reclaim, the old owner's completion is refused with nothing written, and the refusal persists across restarts because it is decided from the file.
- **Every started attempt counts.** Budget consumption is derived from dispatch events and walked through the run's budget lineage; a restart, a re-run, or a new graph revision run with lineage inherits the spend.

## Where the file lives

The store path is chosen by whoever opens it; there is no default yet. Keep it outside any repository that is itself a candidate under verification. The file is created with WAL journaling and `synchronous=FULL`; a `-wal` and a `-shm` file appear beside it while it is open. A file written by a newer schema is refused without being touched; an older one is migrated forward inside a transaction on open.

## What to do after a crash

1. Reopen the file. Nothing else is needed for the ledger itself.
2. Look at the active claims. A claim whose owner is gone is still active, because nothing in M2 detects liveness. Decide by evidence (the owner process no longer exists) and take it over with `Scheduler::reclaim`, which seals the attempt as `failed` with error code `claim_reclaimed`, names the owner it was taken from, and releases the claim. If the owner turns out to be alive, its completion is refused as stale; it must claim again.
3. Apply the retry policy with `apply_retry_policy`. A reclaimed attempt is an infrastructure failure; the policy decides whether another attempt is allowed, and the graph budget and deadline are checked from the file.
4. Cancelled and gated nodes stay as they are. A gated node waits for a decision on the packet it names; a cancelled node is terminal.

## What M2 does not do

- It does not run or verify anything. Attempts are sealed by whoever holds the claim; M3 adds the worker protocol and the software verifier.
- It does not detect a dead controller; reclaim is an operator or controller decision.
- It does not open gate packets on exhaustion itself; the budget module reports `Exhausted { route: gate }` and the gate module builds the packet.
- It is not tamper-proof. A writable local file can be edited with any SQLite tool; the ledger's promises hold for the product's own operations.

## Reproducing the acceptance evidence

Build and test the accepted commit:

```bash
cargo test --locked
```

The durable-recovery suite spawns the test binary as separate processes: one holds a claim transaction until it is killed, two race for one attempt. Set `CHECKSPAN_EVIDENCE_OUT` to a directory to have the inspection test write the views and row counts it observed:

```bash
CHECKSPAN_EVIDENCE_OUT=/tmp/m2-evidence cargo test --locked --test durable_recovery
```

Compare `inspection.json` with the recorded Windows and Linux files under `test-evidence/checkspan/CS-14/`; only timestamps recorded by SQLite differ between machines.
