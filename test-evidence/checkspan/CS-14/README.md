# CS-14 — M2 durable-run-ledger acceptance evidence

**Accepted source:** commit `cb63a14daf5f48476de58dc856283d67e96d9ab1` (CS-14 exercise commit on `codex/checkspan-m1`, also `main`; it contains the CS-08 through CS-13 implementation unchanged plus the test-only recovery exercise and the M2 guide).
**Date:** 2026-09-06. **Recorded by:** Basho Parks' implementation session.

| Evidence class | Environment | Outcome | Where |
| --- | --- | --- | --- |
| `local_native` | Windows 11 x64 (MSVC), rustc/cargo 1.98.0 | passed | `windows-x64-build.log`, `windows-inspection.json`, `windows-retention.txt` |
| `local_native` | Linux x64, WSL2 Ubuntu 26.04, rustc/cargo 1.98.0 | passed | `linux-x64-build.log`, `linux-inspection.json`, `linux-retention.txt` |
| `hosted_ci` | GitHub Actions run 34064484446 (`check (ubuntu-24.04)` job 101570884569, `check (windows-2025)` job 101570884404); `main` run 34064487105 | passed (both jobs; the `main` run also passed) | https://github.com/USS-Parks/Checkspan/actions/runs/34064484446 |
| `hosted_ci` | Every earlier M2 commit | passed | runs 34062341691 (d36e2de), 34062626189 (18247cc), 34062922932 (4ce7a90), 34063343474 (d55a50d), 34063899536 (306989a), 34064253801 (49c0d60) |

## What each run established

**Both native platforms** ran `cargo fmt --check`, `cargo clippy --locked --all-targets -D warnings`, and `cargo test --locked` on the accepted SHA: all 142 tests passed (9 unit, 8 claim ownership, 9 CLI command, 3 CLI smoke, 9 content binding, 15 identity, 7 dependency binding, 15 document validation, 4 durable recovery, 12 graph admission, 9 node contract, 13 outcome, 7 retry and budget, 9 state transition, 13 store). Every store test runs against a real SQLite file; the claim-ownership and durable-recovery suites spawn the test binary as separate operating-system processes.

**Durable recovery (V3), both platforms:** a child process that opens a claim transaction, writes a claim row and a dispatch event, and is then killed leaves no claim, no event, and a dispatch count of zero, and a surviving controller claims with fencing token 1; a claim persists across closing and reopening the file with its token and owner, still counts against the cap, a second controller reclaims it, and the first controller's completion is refused as stale both immediately and after another reopen while the dispatch count and budget consumption read 1; five runs in one file (waiting, failed after a timeout, gated after a rejection exhausted a gate-routed policy with its packet opened, accepted by receipt, cancelled after a claim) replay to views, summaries, dispatch counts, active-claim counts, and event counts that are identical after reopening, exactly one node in the file is `accepted` and it names a stored `accept` receipt, and `inspection.json` is byte-identical between Windows and Linux. `retention.txt` records the ledger size (102400 bytes) and row counts for that five-run file on each platform.

**Hosted:** the matrix ran the same four commands on `ubuntu-24.04` and `windows-2025` and passed for cb63a14 on both `codex/checkspan-m1` (run 34064484446) and `main` (run 34064487105).

## What this does not establish

No worker, verifier, evidence adapter, gate-packet construction, signature check, or external action exists in M2, and none was exercised. Liveness of a controller is not detected; reclaim is a decision. The ledger is a writable local file, not a tamper-proof one.

## Reproduce

```bash
cargo test --locked
```

```bash
CHECKSPAN_EVIDENCE_OUT=/tmp/m2-evidence cargo test --locked --test durable_recovery
```

Compare `/tmp/m2-evidence/inspection.json` with the recorded files; they should be identical.
