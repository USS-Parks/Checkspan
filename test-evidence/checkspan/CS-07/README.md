# CS-07 — M1 contract-explorer acceptance evidence

**Accepted source:** commit `ac639a4d68bc9e2553af26acc189d8d4c45a99b6` (CS-06 implementation, `codex/checkspan-m1`, also `main`).
**Date:** 2026-09-06. **Recorded by:** Basho Parks' implementation session.

| Evidence class | Environment | Outcome | Where |
| --- | --- | --- | --- |
| `local_native` | Windows 11 x64 (MSVC), rustc/cargo 1.98.0 | passed | `windows-x64.txt` |
| `local_native` | Linux x64, WSL2 Ubuntu 26.04, rustc/cargo 1.98.0 installed by `rustup` from `rust-toolchain.toml` | passed | `linux-x64.txt`, `linux-x64-build.log` |
| `hosted_ci` | GitHub Actions run 34059580357: `check (ubuntu-24.04)` job 101557584686, `check (windows-2025)` job 101557584591 | passed | https://github.com/USS-Parks/Checkspan/actions/runs/34059580357 |
| `hosted_ci` (`main` after fast-forward) | GitHub Actions run 34059583565 on the same SHA | passed | https://github.com/USS-Parks/Checkspan/actions/runs/34059583565 |

## What each run established

**Windows native** (`windows-x64.txt`): a release build of the accepted SHA from the canonical checkout (`cargo build --release --locked`, binary SHA-256 `5eef3b7717277a3c3dc74dbef26866e1ade91c6381a581bb9b3d571a3d28add3`), tree clean, then `run-examples.sh`: 3 valid examples exit 0 through both commands; 9 invalid examples exit 1 with the expected stage (`schema`, `admission`, `duplicate_key`, `header`); a missing file exits 3; no arguments and an unknown flag exit 2. Earlier in the session, `cargo fmt --check`, `cargo clippy -D warnings`, and `cargo test --locked` (83 tests) passed on this SHA on Windows (CS-06 entry in the development log).

**Linux native** (`linux-x64-build.log`, `linux-x64.txt`): a fresh `git clone` of the public repository from GitHub inside WSL2 Ubuntu, `git checkout ac639a4d…`, toolchain 1.98.0 installed by `rustup toolchain install` from the pinned file, then `cargo fmt --check` passed, `cargo clippy --locked --all-targets -D warnings` passed, `cargo test --locked` passed all 83 tests (8 unit, 9 CLI command, 3 CLI smoke, 14 identity, 15 document validation, 12 graph admission, 9 node/evidence, 13 outcome), `cargo build --release --locked` produced a binary with SHA-256 `ed7b382b485e6724c8cfe55eae9c603b9e1e01b893aee9c49f8f1545a9bdcd9d`, and `run-examples.sh` produced the same 27 result lines as Windows: identical exit codes, stages, and output SHA-256 digests for every example. WSL2 is a Linux kernel and userland; it is native Linux execution, not a hosted runner and not a container of the Windows build.

**Hosted** (run 34059580357): both matrix jobs ran `cargo fmt --check`, `cargo clippy --locked --all-targets -D warnings`, `cargo test --locked`, and `cargo build --locked --release` on the accepted SHA; the job logs show all 8 test binaries passing (83 tests) on each runner. The hosted matrix also passed on every earlier M1 commit (runs 34056847082, 34057533885, 34058130081, 34058737149, 34059107159).

## What this does not establish

No worker, run store, credential path, network client, or external action exists in M1, and none was exercised. Nothing here is a claim about executing work, verifying software, retrieving corpora, calling models, or accepting operator decisions; those are M2 and later. `run-examples.sh` is evidence tooling, not product code.

## Reproduce

Build the accepted SHA with `cargo build --release --locked` and run:

```bash
bash test-evidence/checkspan/CS-07/run-examples.sh target/release/checkspan /tmp/my-run.txt
```

The 27 result lines after the header should match `windows-x64.txt` and `linux-x64.txt` exactly.
