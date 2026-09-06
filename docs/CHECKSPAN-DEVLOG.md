# Checkspan — development log

## Execution state

- Canonical plan: [CHECKSPAN-PSPR](../PLANNING/CHECKSPAN-PSPR.md), Draft 0.2.
- Product: Checkspan, an independent exploration.
- Repository: [USS-Parks/Checkspan](https://github.com/USS-Parks/Checkspan).
- Implementation authorization: **full STS approved by Basho on 2026-09-06** ("Approved for full STS now"). Execution proceeds sequentially and halts at every explicit stop in PSPR §0.3; the first stop is the M1 boundary after CS-07.
- Implementation branch: `codex/checkspan-m1`. Per Basho's instruction of 2026-09-06, every completed prompt is committed on that branch, fast-forwarded into `main`, and both refs are pushed.
- Current prompt: **CS-04 complete; CS-05 next.**
- Implemented product behavior: `checkspan --help` / `--version`; library-level contracts for graphs, nodes, evidence, attempts, verifier receipts, gate packets, gate decisions, and node views, with bundled schemas and record-level binding checks (no CLI exposure yet).

## DOC-00 — Name, repository wiring, and review draft

**Date:** 2026-09-06.  
**Scope authorized:** choose a new independent product name, connect the provided repository, and draft a granular PSPR for review.  
**Status:** complete; exploration papers and draft PSPR published to main.  
**Documentation commit:** 6bc2bb888147123c4053fff0bf4a05bf9354cf1f

**Changes:** introduced Checkspan and its standalone positioning; prepared the current one-pager, schema notes, and README; retained the original v0.1 papers unchanged; drafted the 36-prompt, five-milestone PSPR and verification ledger. Added basic text/ignore conventions for the new repository.

**Verification:** original document hashes checked before writing; current document read-back and internal links checked; prompt IDs, ordering, per-prompt objectives/gates, milestone ranges, and unapproved state reviewed. GitHub metadata identified an empty public repository with default branch `main` before wiring.

**Not performed:** no Cargo package, runtime, worker, verifier, integration, CI workflow, credential setup, or release. No implementation/live gate is marked passed.

**Working-copy discipline:** use the user-named folder directly. No secondary checkout/worktree or dependency tree is required. Temporary drafting payloads remain outside the repository and are not product artifacts.

## Prompt entry format

For every approved prompt, append: prompt ID and approved scope; before/source SHA; changed paths; verification commands and outcomes; native/hosted/operator evidence references; acceptance status; implementation commit; remote SHA; open blockers; storage/worktree closeout.

Do not replace historical failures with a later success. Record retries and superseding evidence separately. Keep prompt acceptance separate from release authorization.

## DOC-01 - Publication closeout

The initial documentation commit 6bc2bb888147123c4053fff0bf4a05bf9354cf1f was pushed to origin/main and the remote SHA was verified with git ls-remote. Staged diff checking and the configured no-slop commit/push hooks passed. The 36-prompt roster and all required prompt fields were checked; internal document links resolve.

The only retained worktree is the canonical working folder on main, used for this exploration and future approved work. No secondary worktree, build output, dependency tree, or unpublished implementation exists. Temporary drafting payloads remain outside the repository; no deletion was performed.

CS-01 through CS-36 remain not approved and not started. This closeout changes documentation only. Review and explicit STS approval are still required.

## DOC-02 — Correct the omitted multi-agent RAG scope

**Date:** 2026-09-06.  
**Source commit:** df18707d0ae5cea46d8193b670ec468eedfa2650.  
**Review trigger:** Basho noted that the PSPR did not mention multi-agent RAG.  
**Scope:** documentation revision only; no implementation or model/corpus access approval.

Draft 0.1 parked RAG ingestion, model execution, and parallel workers. Draft 0.2 corrects that omission: a proposed combined M3 pilot now includes distinct requirements/implementation retrieval agents, synthesis, challenge, mechanical evidence checking, and explicit human semantic assessment. First-pilot placement is the draft recommendation; no unanswered scope question is treated as approval.

**Changed paths:** PLANNING/CHECKSPAN-PSPR.md; CHECKSPAN-ONE-PAGER.md; CHECKSPAN-SCHEMA-NOTES.md; README.md; docs/CHECKSPAN-DEVLOG.md; docs/CHECKSPAN-VERIFICATION.md.

**Roster:** original CS-01–CS-36 identifiers preserved; CS-R01–CS-R12 inserted after CS-24 and before CS-25, giving 48 prompts and five milestone approval cuts. Added V9–V11 and 12 RAG review cases. Active product/schema papers are v0.3. Historical v0.1 files remain unchanged.

**Document gate:** exact 48-ID sequence and five required fields per prompt; dependency ordering; active-document links and version/scope consistency; Markdown whitespace and historical hashes. Product gates remain not run. This entry's documentation commit is identified by Git history; final publication SHA is reported separately after remote verification.

**Execution state:** all implementation prompts remain unapproved and unstarted. No model runtime, corpus, credential, spend, agent worker, generated build tree, or extra worktree was introduced. Temporary drafting data remains outside the repository; no deletion is authorized or performed.

## CS-01 — Establish the reproducible Rust baseline

**Date:** 2026-09-06.  
**Approved scope:** full STS (Basho, 2026-09-06); CS-01 is the first prompt of M1.  
**Source SHA before work:** 8de894f65863e8ff95d1dad0d9c2415e7d606596 (main).  
**Branch:** `codex/checkspan-m1`.

**Changed paths:** `Cargo.toml`, `Cargo.lock`, `rust-toolchain.toml`, `deny.toml`, `src/lib.rs`, `src/cli.rs`, `src/main.rs`, `tests/cli_smoke.rs`, `.github/workflows/ci.yml`, `docs/CHECKSPAN-DEPENDENCIES.md`, this log, the verification ledger.

**Prerequisites verified (V0):** rustc/cargo 1.98.0 stable on `x86_64-pc-windows-msvc` with rustfmt and clippy; MSVC linker confirmed by a scratch `cargo build` outside the repository; git 2.54.0; gh 2.92.0 authenticated as USS-Parks; 279 GB free on the working volume; cargo-deny 0.19.7 and cargo-audit 0.22.1 present. Clean tree on `main` before branching.

**Commands and outcomes (local_native, Windows x64):**

| Command | Outcome |
| --- | --- |
| `cargo generate-lockfile` then `cargo build --locked` | passed; 22 crates |
| `cargo fmt --all -- --check` | passed |
| `cargo clippy --locked --all-targets -- -D warnings` | passed after scoping `missing_docs` to the library crate (first run failed on the binary and test crates) |
| `cargo test --locked` | passed, 3 tests: `--version` output, `--help` output, unknown-flag exit code 2 |
| `cargo deny check` | advisories, bans, licenses, sources ok (own crate's pending license ignored via `[licenses.private]`) |
| `cargo audit` | no advisories against 22 dependencies |

**Not claimed:** hosted CI behavior. The workflow definition was reviewed and its four commands were run locally; the hosted matrix result is recorded at CS-07 after the branch is pushed.

**Acceptance:** CS-01 gate passed locally. Implementation commit SHA is recorded in the CS-02 entry.  
**Open blockers:** none.  
**Storage/worktree:** canonical checkout only; `target/` generated locally and ignored.

## CS-02 — Define identities and graph envelopes

**Date:** 2026-09-06.  
**Source SHA before work:** 09302940b5ab9612ed2cd3d25738589d0fe9db43 (CS-01 implementation commit, `codex/checkspan-m1`).  
**CS-01 hosted evidence:** GitHub Actions run 34056847082 on that SHA, `ubuntu-24.04` and `windows-2025`, conclusion success (recorded here as `hosted_ci` for CS-01; M1 hosted acceptance is still CS-07).  

**Changed paths:** `schemas/v1/{common,node-ref,graph-ref,node-spec,graph-spec,graph-run}.schema.json`; `src/contracts/{mod,ids,record,registry,graph}.rs`; `src/lib.rs`; `tests/contract_identity.rs`; `tests/fixtures/contracts/{valid,invalid}/*.json` (4 valid, 20 invalid); `Cargo.toml`, `Cargo.lock`; `docs/CHECKSPAN-DEPENDENCIES.md`; this log; the verification ledger.

**Design as implemented:** every top-level record is self-describing through `record` and `schema_version`; parsing is header-first, so an unsupported version is reported before any shape error. Identifiers share one grammar (`^[a-z0-9][a-z0-9_.-]{0,127}$`); revisions start at 1; display names are optional and never identity. `NodeRef` is `{graph_id, node_id, revision}` and equality covers all three. `GraphSpec` targets are full `NodeRef`s that must resolve to a node in the same graph at its exact revision; `supersedes` must name an earlier revision of the same graph. `GraphRun` binds `run_id` to an exact `GraphRef` and may name an earlier run as `budget_lineage_ref`, never itself. Schema `$id`s live under the reserved `checkspan.invalid` host and are served only by the bundled registry.

**Commands and outcomes (local_native, Windows x64):**

| Command | Outcome |
| --- | --- |
| `cargo fmt --all -- --check` | passed (after one `cargo fmt --all`) |
| `cargo clippy --locked --all-targets -- -D warnings` | passed (one `type_complexity` finding fixed with a type alias) |
| `cargo test --locked` | passed: 3 unit (identifier grammar, revision floor, timestamp shape), 3 CLI smoke, 14 contract-identity tests |
| `cargo deny check` | advisories, bans, licenses, sources ok |
| `cargo audit` | no advisories against 95 lockfile entries |

**Gate evidence (V1/V2):** same local slug distinguished across graphs and revisions (`HashSet` of three refs, Display output); two runs of one graph revision distinct; display-name change leaves `GraphRef`/`NodeRef` equal; `schema_version: 2` with an extra field rejects as `UnsupportedVersion`, not a shape error; wrong/unknown/missing record headers reject; missing, empty, foreign, unknown, revision-mismatched, and duplicate targets reject; `supersedes` foreign/not-earlier reject; self-lineage rejects; all six bundled schemas are valid Draft 2020-12 and compile offline; `$ref`s to unbundled `https://`, `file://`, and unknown in-namespace IDs fail to compile rather than fetch. Each valid fixture passes schema validation, typed parse, identity check, and a serialize/parse round trip.

**Not claimed:** CLI exposure of validation (CS-06), bounded parsing and duplicate-key detection (CS-05), node contract bodies (CS-03), hosted matrix for this commit (queued on push; recorded at CS-07).

**Acceptance:** CS-02 gate passed locally. Implementation commit SHA is recorded in the CS-03 entry.  
**Open blockers:** none.

## CS-03 — Define immutable node and evidence contracts

**Date:** 2026-09-06.  
**Source SHA before work:** 918a8431ec8d46b19029e3ecbbb77e785b25716c (CS-02 implementation commit; also `main` after the fast-forward merge Basho requested).  

**Changed paths:** `schemas/v1/common.schema.json` (adds `version`, `digest`, `policy_ref`, `type_ref`, `verifier_ref`); `schemas/v1/node-spec.schema.json` (full contract with kind-conditional rules); `schemas/v1/evidence-ref.schema.json` (new); `src/contracts/ids.rs` (adds `Ident`, `Version`, `Digest`, `Exactly<V>`); `src/contracts/node.rs`, `src/contracts/evidence.rs` (new); `src/contracts/{graph,mod,registry}.rs`; `tests/common/mod.rs` (shared helpers); `tests/node_contracts.rs` (new); `tests/contract_identity.rs`; fixtures regenerated with full node bodies (`contracts/` 4 valid + 20 invalid, `nodes/` 5 valid + 35 invalid, `evidence/` 2 valid + 6 invalid); this log; the verification ledger; the dependency record.

**Design as implemented:** `kind` is one of `task`, `check`, `human_gate`, `sink`. A `check` or `sink` needs at least one dependency; a `human_gate` needs exactly one `human` evidence port and no other kind may declare one. Dependencies name an exact `NodeRef`, an output port, and the expected `TypeRef` (`schema_id`, `version`, `digest`). Evidence ports declare `kind` (`rag`, `human`, `logs`, `code`, `proofs`), `expected_type`, `required`, a non-empty `allowed_source_scope`, and a `handling_policy_ref`; declaration does not enforce the boundary. `acceptance` pins `claim`, a non-empty `required_checks` list, the `verifier` (`id`, `version`, `digest`), and `policy_ref`. `retry_policy` is inline with a fixed format `version` of 1 (the `Exactly<1>` marker rejects any other value while parsing) and defaults to three attempts, all failure classes, gate on exhaustion. `authority_policy_ref` is mandatory on every node so authority is declared before dispatch. `resource_scope` lists `exclusive`/`shared` claims. `supersedes` must name an earlier revision of the same node. `EvidenceRef` binds `port_name`, `locator`, `subject`, `version`, `content_digest`, `provenance` (`producer`, optional `attestation_ref`), `policy_ref`, and optional `valid_until`. No record or schema property can express a status, verdict, or acceptance.

**Commands and outcomes (local_native, Windows x64):**

| Command | Outcome |
| --- | --- |
| `cargo fmt --all -- --check` | passed |
| `cargo clippy --locked --all-targets -- -D warnings` | passed |
| `cargo test --locked` | passed: 29 tests (3 unit, 3 CLI smoke, 14 contract identity, 9 node/evidence contract) |
| `cargo deny check` / `cargo audit` | not rerun; no dependency change since CS-02 |

**Gate evidence (V1/V2):** mandatory checks and inputs cannot disappear (empty or missing `required_checks`, missing `verifier`, an extra `skip_checks` field, missing `result_type`, missing port `required` or `allowed_source_scope`, empty scope, missing `authority_policy_ref` all reject); wrong port/output types reject (unknown node kind, non-identifier port name, missing `expected_type`, unknown port kind, malformed digests on verifier and result type, version 0, unknown resource access); unsupported policy versions reject (`retry_policy.version: 2` fails with "not supported; this build supports 1", zero `max_attempts`, unknown failure class and exhaustion routing, policy ref version 0); kind-conditional rules are enforced by both the schema (`if`/`then`, `contains` with `minContains`/`maxContains`) and the Rust check; rules the schema cannot express (duplicate dependency, port name, resource; `supersedes` on another node or a later revision; whitespace-only prompt) reject in the Rust check on schema-valid documents; every valid node fixture contains no outcome key, the node schema offers none, and a document carrying `status` rejects; retry defaults are explicit after parsing; evidence fixtures round-trip and malformed digest, timestamp, unknown field, and missing provenance reject.

**Not claimed:** cross-node type compatibility, cycles, and hidden proof dependencies (CS-06); attempts, receipts, and decisions (CS-04); actual evidence resolution or scope enforcement (CS-16); hosted matrix for this commit (queued on push; recorded at CS-07).

**Acceptance:** CS-03 gate passed locally. Implementation commit SHA is recorded in the CS-04 entry.  
**Open blockers:** none.

## CS-04 — Define attempts, receipts, and gate decision records

**Date:** 2026-09-06.  
**Source SHA before work:** abe323dfe93c910d3d175d4dd80f680a4743a947 (CS-03 implementation commit; also `main` after the fast-forward merge).  

**Changed paths:** `schemas/v1/{attempt,verifier-receipt,gate-packet,gate-decision,node-view}.schema.json` (new); `schemas/v1/common.schema.json` (adds `attempt_number`, `attempt_ref`, `receipt_ref`, `provenance_level`, `text`); `src/contracts/{attempt,gate,view}.rs` (new); `src/contracts/{ids,record,registry,mod}.rs`; `tests/outcome_contracts.rs` (new); `tests/fixtures/outcomes/` (28 valid, 67 invalid); this log; the verification ledger; the dependency record.

**Design as implemented:** `Attempt` records run, node, attempt number, owner, timestamps, an optional repair hint, the input manifest and dependency receipts frozen at dispatch, the result and produced evidence, an `execution` record, and the `verifier_receipt` reference once one exists; it has no acceptance field. `execution.outcome` is `completed`, `failed`, `timed_out`, or `cancelled`; `VerifierReceipt.verdict` is `accept`, `reject`, or `undecidable`; the two vocabularies are disjoint and each enum rejects the other's words. A receipt binds an exact `AttemptRef`, subject, result digest, context digest, verifier identity with digest, policy, verdict, reason fields, validity, and a `provenance` with level `local_controller`, `authenticated_upstream`, or `signed_operator`; `reject` and `undecidable` need a `reason_code`, `accept` cannot carry a `retry_hint`. `VerifierReceipt::binds(&Attempt)` requires the exact attempt, completed execution, and an equal result digest. `GatePacket` carries purpose (`resolve_work`, `assess_claims`, `authorize_action`), sealed context references, a `subject_digest`, the offered options (restricted per purpose), the exact action for authorization packets, the authority policy, and an expiry. `GateDecision` binds a packet by id and digest, names the authority and its authentication level, carries a typed decision (`retry`, `revise`, `cancel`, `approve_action`, `deny_action`, `record_assessment`), an assessment payload exactly for `record_assessment`, and a scope that must name the action for action decisions. `GateDecision::binds(&GatePacket)` checks id, digest, offered option, run, node, and action; `authorizes(action)` is true only for `approve_action` on that exact action; `is_denial()` covers `deny_action` and `cancel`. `NodeView` has status `open`, `running`, `accepted`, `rejected`, `failed`, `gated`, or `cancelled`; `accepted` and `rejected` must name the receipt that determined them and the receipt must be about the same node and run; no other status may carry one. All five are self-describing records parsed header-first with bundled Draft 2020-12 schemas that encode the same conditionals.

**Commands and outcomes (local_native, Windows x64):**

| Command | Outcome |
| --- | --- |
| `cargo fmt --all -- --check` | passed |
| `cargo clippy --locked --all-targets -- -D warnings` | passed |
| `cargo test --locked` | passed: 42 tests (3 unit, 3 CLI smoke, 14 contract identity, 9 node/evidence, 13 outcome) |
| `cargo deny check` / `cargo audit` | not rerun; no dependency change since CS-02 |

**Gate evidence (V1/V2):** records cannot express acceptance without receipt bindings (attempt with a receipt but no result, or with failed/timed-out execution, or a receipt about another node or run; view `accepted`/`rejected` without a receipt or attempt, or with a receipt about another node or run; receipt on `open`/`running` views); an authentic denial remains a denial (the `deny_action` fixture is well-formed, binds to its packet, `is_denial()` is true, `authorizes()` is false for the exact action; `approve_action` authorizes only the exact action and target; `cancel` is a denial); a timeout cannot parse as a verdict (`timed_out`, `timeout`, `failed`, `completed` all fail as `verdict`; `reject`/`accept` fail as `execution.outcome`; `verdict`/`outcome` fields on the wrong record reject; the two enums' string sets are disjoint); a decision bound to a changed packet digest, a retargeted action, a packet that did not offer the decision, or another node fails to bind; `record_assessment` requires its payload and other decisions cannot carry one; packets offer only purpose-allowed options and name an action exactly for authorization; a decision record cannot be read as a receipt and no status vocabulary includes `approved`/`proved`. Golden fixtures cover all 4 execution outcomes, 3 verdicts, 3 provenance levels, 3 purposes, 6 decisions, 3 assessment outcomes, and 7 statuses, and the coverage is asserted.

**Not claimed:** any state transition (CS-10); receipt admission against a verifier identity or trust policy (CS-19); signature verification of decisions (CS-22); expiry against a real clock; hosted matrix for this commit (queued on push; recorded at CS-07).

**Acceptance:** CS-04 gate passed locally. Implementation commit SHA is recorded in the CS-05 entry.  
**Open blockers:** none.
