# Checkspan — development log

## Execution state

- Canonical plan: [CHECKSPAN-PSPR](../PLANNING/CHECKSPAN-PSPR.md), Draft 0.2.
- Product: Checkspan, an independent exploration.
- Repository: [USS-Parks/Checkspan](https://github.com/USS-Parks/Checkspan).
- Implementation authorization: **full STS approved by Basho on 2026-09-06** ("Approved for full STS now"). Execution proceeds sequentially and halts at every explicit stop in PSPR §0.3; the first stop is the M1 boundary after CS-07.
- Implementation branch: `codex/checkspan-m1`. Per Basho's instruction of 2026-09-06, every completed prompt is committed on that branch, fast-forwarded into `main`, and both refs are pushed.
- Current prompt: **CS-07 complete. M1 (CS-01–CS-07) is accepted on native Windows, native Linux, and the hosted matrix. Execution is STOPPED at the M1 milestone boundary pending Basho's explicit M2 approval; CS-08 is not started.**
- Implemented product behavior: `checkspan validate <file>` and `checkspan inspect <file>` over any supported record, with graph admission (dependency resolution, port and type compatibility, cycles, hidden proof dependencies, external imports, gate wait chains, target closure, deterministic order) for graph documents; stable JSON output and exit codes; no dispatch, run store, or writes.

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

## CS-05 — Implement bounded offline document validation

**Date:** 2026-09-06.  
**Source SHA before work:** 3eecbf06344080a4c15c409fa9ad5fa984fb4d26 (CS-04 implementation commit; also `main` after the fast-forward merge).  

**Changed paths:** `src/validation/{mod,parse,schema}.rs` (new); `src/lib.rs`; `src/contracts/{record,mod}.rs` (public `RecordHeader` / `read_header`); `Cargo.toml` (`jsonschema` promoted to a runtime dependency); `deny.toml` (allow `MIT-0`, `Zlib`); `tests/document_validation.rs` (new); this log; the verification ledger; the dependency record.

**Design as implemented:** `validate(bytes, &Limits)` runs fixed stages in order and stops at the first that fails: size (default 4 MiB), UTF-8, strict parse, header, schema, shape, contract. The strict parser is a serde visitor that builds a `serde_json::Value` while enforcing a nesting limit (default 64 containers), rejecting any repeated key inside one object, and rejecting trailing content; every parse failure carries the JSON pointer of the container or key involved. The header stage requires a JSON object with `record` and `schema_version` and matches them against the registry, listing the supported records on failure. The schema stage uses validators compiled once per process from the bundled corpus through an in-crate retriever that serves only bundled `$id`s; `jsonschema` is built with `default-features = false`, so no HTTP, file, or TLS resolver exists in the binary. A frozen keyword set (`SUPPORTED_KEYWORDS`) is enforced before compilation: any schema using another Draft 2020-12 keyword is refused. Schema diagnostics carry the instance JSON pointer; contract diagnostics carry the containing field (`/targets`, `/nodes/<i>`, `/status`, ...). A valid document yields a `ValidatedRecord` of the matching kind. Nothing in the module touches the filesystem or the network.

**Commands and outcomes (local_native, Windows x64):**

| Command | Outcome |
| --- | --- |
| `cargo fmt --all -- --check` | passed |
| `cargo clippy --locked --all-targets -- -D warnings` | passed |
| `cargo test --locked` | passed: 62 tests (8 unit incl. 5 parser, 3 CLI smoke, 14 identity, 9 node/evidence, 13 outcome, 15 document validation) |
| `cargo deny check` | advisories, bans, licenses, sources ok (after allowing `MIT-0` and `Zlib`, which entered the runtime graph with `jsonschema`) |
| `cargo audit` | no advisories against 95 lockfile entries |

**Gate evidence (V1/V2):** oversized input rejects at the size stage with nothing interpreted; a 70-deep nesting inside an otherwise valid graph rejects at the depth stage with the path `/nodes/0/display_name/0/0...` and reaches the schema stage only when the limit is raised; duplicate keys reject with the repeated key's path (`/graph_id`, `/nodes/0/id`); empty, malformed, unterminated, trailing-content, and non-UTF-8 inputs reject at the syntax stage; non-object documents and missing or unsupported headers reject at the header stage with the supported list; schema diagnostics carry field paths (`/graph_id`, `/nodes/0`, `/verdict`); contract diagnostics name the containing field; every valid fixture of every record kind yields its typed record and every invalid top-level fixture is rejected by exactly one stage; `https://`, `http://`, `file://`, `../` traversal, unknown in-namespace, and missing-fragment references all fail to compile while a bundled reference compiles and validates; `Cargo.lock` contains no `reqwest`, `hyper`, `rustls`, `aws-lc-rs`, `tokio`, `ureq`, or `url`; the bundled corpus uses exactly the frozen keyword set and every frozen keyword has an inline positive and negative case (`type`, `properties`, `required`, `additionalProperties`, `enum`, `const`, `allOf`, `if`/`then`/`else`, `not`, `items`, `minItems`, `contains`/`minContains`/`maxContains`, `minimum`, `maximum`, `minLength`, `maxLength`, `pattern`, `format`, `$ref`/`$defs`); `$dynamicRef`, `unevaluatedProperties`, `dependentSchemas`, `patternProperties`, `$anchor`, `anyOf`, `oneOf`, `uniqueItems`, and `$comment` are refused before compilation while property names that merely look like keywords are not.

**Not claimed:** CLI exposure (`validate`/`inspect` are CS-06); cross-node graph admission (CS-06); hosted matrix for this commit (queued on push; recorded at CS-07).

**Acceptance:** CS-05 gate passed locally. Implementation commit SHA is recorded in the CS-06 entry.  
**Open blockers:** none.

## CS-06 — Implement graph admission and read-only commands

**Date:** 2026-09-06.  
**Source SHA before work:** f7faf4118649054f568aa62922eaf2ad187aaf2f (CS-05 implementation commit; also `main` after the fast-forward merge).  

**Changed paths:** `src/graph/mod.rs` (new); `src/cli.rs` (rewritten: `validate` and `inspect`); `src/lib.rs`; `src/validation/mod.rs` (adds `Stage::Admission`); `Cargo.toml` (`autoexamples = false`); `examples/README.md`, `examples/graphs/{review-pilot,research-pilot}.json`, `examples/records/graph-run.json`, `examples/invalid/*.json` (9); `tests/graph_admission.rs`, `tests/cli_commands.rs` (new); `tests/fixtures/graphs/` (4 valid, 17 invalid); this log; the verification ledger; the dependency record.

**Design as implemented:** `graph::admit(&GraphSpec)` composes the envelope identity rules and every node's contract rules with the cross-node rules: dependencies must name this graph (a foreign graph is an unsupported external import), an existing node, and that node's exact revision; a node cannot depend on itself; the only output port is `result`; a dependency's `expected_type` must equal the producer's `result_type` including digest; a `proofs` port names in-graph sources as `node:<id>`, each of which must be a declared dependency, and any other proof source is an unsupported import; a `human` port on a `human_gate` names the nodes it resolves as `node:<id>`, and a gate joined to a resolved node by an acceptance chain in either direction (or resolving itself) is a gate wait cycle. Cycles are found by Kahn's algorithm with document-order tie-breaking and reported once with a deterministic path. Admission returns every error in document order, or an `AdmittedGraph` with the topological `order`, the `required` closure of the targets, the `optional` remainder, and gate bindings. It reads the graph only. The CLI reads one file (checking size from metadata before reading), runs the validation pipeline, adds admission for graph documents, and prints one JSON object with `command`, `path`, `valid`, `record`, `schema_version`, `schema_id`, and staged, path-bearing `diagnostics`; `inspect` adds a `graph` description or an `identity` summary. Exit codes: 0 valid, 1 rejected, 2 usage, 3 unreadable file. Output keys are sorted, so output is byte-stable across runs.

**Commands and outcomes (local_native, Windows x64):**

| Command | Outcome |
| --- | --- |
| `cargo fmt --all -- --check` | passed |
| `cargo clippy --locked --all-targets -- -D warnings` | passed |
| `cargo test --locked` | passed: 83 tests (8 unit, 3 CLI smoke, 9 CLI command, 14 identity, 15 document validation, 12 graph admission, 9 node/evidence, 13 outcome) |
| `cargo deny check` / `cargo audit` | not rerun; no dependency change since CS-05 |

**Gate evidence (V1/V2):** the chain-with-gate graph admits with order `cs_patch, cs_ci, cs_ci_gate, cs_packet`, required closure `cs_patch, cs_ci, cs_packet`, the gate optional and bound to `cs_ci`; the six-node diamond admits with both branches before the join and everything required; an isolated target admits with the unrelated side nodes reported optional; node revisions independent of the graph revision admit; admission is deterministic (equal results across runs); unresolved, foreign-graph, revision-mismatched, and self dependencies reject with the exact reference; an unknown output port and a mismatched or digest-drifted result type reject; a two-node and a three-node cycle each produce exactly one `Cycle` error with a deterministic path (`cs_patch -> cs_ci -> cs_patch`); hidden, unresolved, and external proof sources reject; a gate that depends on the node it resolves, a node that depends on its gate, a gate that resolves itself, and a gate that resolves an unknown node reject; identity and node-contract errors surface through admission in document order with `/nodes/<i>` paths. Through the binary: the three valid examples exit 0 with empty diagnostics; all nine invalid examples exit 1 with the expected single rejecting stage (`admission`, `duplicate_key`, `header`, `schema`) and a path on every diagnostic; an unreadable file exits 3 with an `io` diagnostic; `inspect` reports order, closure, gates, and per-node deps and checks, or `identity` for a run or receipt; `inspect` on an invalid document exits 1 without a `graph` section; running both commands in a scratch directory creates no files and leaves the input bytes unchanged, and no `.checkspan` directory appears in the repository; output is identical across two runs with empty stderr; no subcommand prints help and exits 2.

**Not claimed:** native Linux behaviour and the hosted matrix for this commit (queued on push; both recorded at CS-07); any dispatch, run store, or state transition (M2).

**Acceptance:** CS-06 gate passed locally. Implementation commit SHA is recorded in the CS-07 entry.  
**Open blockers:** none.

## CS-07 — Accept the contract-explorer milestone

**Date:** 2026-09-06.  
**Accepted source SHA:** ac639a4d68bc9e2553af26acc189d8d4c45a99b6 (CS-06 implementation commit; `codex/checkspan-m1` and `main`).  
**Scope:** reports and docs only. No product code changed; no code defect was found, so no implementation prompt was reopened.

**Changed paths:** `docs/CHECKSPAN-M1-GUIDE.md` (new beginner instructions); `README.md` (status and entry points); `test-evidence/checkspan/CS-07/{README.md,run-examples.sh,windows-x64.txt,linux-x64.txt,linux-x64-build.log}` (new); this log; the verification ledger; the dependency record.

**Native Windows x64 (local_native, passed):** release build of the accepted SHA from the clean canonical checkout, binary SHA-256 `5eef3b77…28add3`; `run-examples.sh` shows the 3 valid examples exit 0 through both commands, the 9 invalid examples exit 1 with their expected stage, a missing file exits 3, no arguments and an unknown flag exit 2. The V1 ladder (fmt, clippy `-D warnings`, 83 tests) passed on this SHA in the CS-06 entry.

**Native Linux x64 (local_native, passed):** WSL2 Ubuntu 26.04, kernel 6.6.87.2; fresh `git clone` of the public repository from GitHub, `git checkout ac639a4d…`, toolchain 1.98.0 installed by `rustup toolchain install` from the pinned file; `cargo fmt --check` passed, `cargo clippy --locked --all-targets -D warnings` passed, `cargo test --locked` passed all 83 tests, `cargo build --release --locked` produced binary SHA-256 `ed7b382b…bdcd9d`; `run-examples.sh` produced the same 27 result lines as Windows, with identical exit codes, stages, and output digests for every example. The evidence files contain no tokens or secrets (scanned).

**Hosted matrix (hosted_ci, passed):** GitHub Actions run 34059580357 on ac639a4d…, jobs `check (ubuntu-24.04)` 101557584686 and `check (windows-2025)` 101557584591, every step successful, all 8 test binaries (83 tests) passing on each runner; run 34059583565 on `main` at the same SHA also passed. Every earlier M1 commit passed the same matrix: 34056847082 (0930294), 34057533885 (918a843), 34058130081 (abe323d), 34058737149 (3eecbf0), 34059107159 (f7faf41).

**Gate (V0–V2):** correct checkout, branch, and SHA recorded; clean tree; the CLI-only instructions in the M1 guide and `examples/README.md` reproduce every expected status and exit code on both platforms; no worker, run store, credential path, or network client exists (the build contains no HTTP or TLS crate, and both commands are shown to create no files). Contract and graph gates (V2) are the committed test suites from CS-02–CS-06, passing natively on both platforms and on the hosted matrix.

**Not claimed:** anything beyond offline validation and admission. No execution, storage, verifier, retrieval, model, signature, GitHub-evidence, or packaging claim is made.

**Storage and worktree closeout (removal requires Basho's authorization; none performed):**

| Item | Location | Purpose | Retirement condition |
| --- | --- | --- | --- |
| Canonical checkout | `C:\Users\17076\Documents\Codex\Work Graph Project`, `codex/checkspan-m1` (= `main`) | Sequential implementation lane | Retained |
| Build output | `target/` in the canonical checkout, about 1.4 GB, git-ignored | Debug and release builds, test binaries | May be deleted at any time; regenerated by `cargo build` |
| Linux evidence clone | WSL2 Ubuntu, `/home/basho/checkspan-m1`, detached at ac639a4d…, plus its `target/` | Native Linux acceptance evidence for CS-07 | After Basho has reviewed M1 acceptance |
| Linux toolchain | WSL2 Ubuntu, `~/.rustup` and `~/.cargo` (rustup 1.29.1, toolchain 1.98.0 minimal profile with rustfmt and clippy) | Building on native Linux | Retained for later milestone acceptance unless Basho says otherwise |

No git worktree other than the canonical checkout is registered (`git worktree list`). No unpublished commits: `codex/checkspan-m1` and `main` both point at the CS-07 closeout commit after this entry is pushed. Generated fixtures and examples are committed source; scratch generators and evidence scripts used during the session live outside the repository except `run-examples.sh`, which is committed as evidence tooling.

**Acceptance:** M1 accepted with native Windows, native Linux, and hosted evidence on ac639a4d…. **Execution stops here.** CS-08 (M2) requires Basho's explicit approval; nothing in this entry, in elapsed time, or in the earlier full-STS instruction substitutes for that stop, because the milestone boundary is an explicit stop in the approved PSPR.
